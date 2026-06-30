use crate::db::query_hydration::load_ruleset_history;
use crate::db::query_insert::{reserve_conn_id_block, save_config_row, save_ruleset_row};
use crate::db::schema::init_schema;
use crate::observability::sinks::sink_ecam;
use crate::observability::sinks::sink_ecam::{Origin, Severity};
use rusqlite::{Connection, params};
use shared::types::record_types::EvaluationRecord;
use shared::types::routing::filter::ruleset::RulesetHistoryEntry;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tracing::info;

#[derive(Clone)]
pub struct DbHandle {
    pub(crate) tx: mpsc::Sender<FilterDbCmd>,
    pub path: Arc<String>,
}

pub enum FilterDbCmd {
    InsertRecord(EvaluationRecord),
    SaveRuleset {
        port: u16,
        version: String,
        json: String,
        ts_ns: i64,
    },
    SaveConfig {
        external_port: u16,
        upstream_port: u16,
        json: String,
        ts_ns: i64,
    },
    /// Reserve the next block of connection ids; replies with `(base, ceiling)`.
    ReserveConnIds {
        reply: oneshot::Sender<(u64, u64)>,
    },
    /// Load a port's full ruleset publish history (newest first).
    LoadRulesetHistory {
        port: u16,
        reply: oneshot::Sender<Vec<RulesetHistoryEntry>>,
    },
    Shutdown,
}

pub fn init_db(path: &str) -> anyhow::Result<DbHandle> {
    {
        // pre-init schema
        let conn = Connection::open(path)?;
        init_schema(&conn)?;
    }

    // spawn actor
    let (tx, mut rx) = mpsc::channel::<FilterDbCmd>(20_000);
    let db_path = path.to_string();
    let db_path_clone = db_path.clone();

    tokio::spawn(async move {
        // one blocking connection for the actor
        let mut conn = Connection::open(&db_path).expect("open db");

        // buffers
        let mut rec_buf: Vec<EvaluationRecord> = Vec::with_capacity(512);

        loop {
            tokio::select! {
                Some(cmd) = rx.recv() => {
                    match cmd {
                        FilterDbCmd::InsertRecord(rec) => {
                            rec_buf.push(rec);
                            if rec_buf.len() >= 512
                                && let Err(e) = flush_records(&mut conn, &mut rec_buf) {
                                    sink_ecam::record(
                                        Severity::Error,
                                        Origin::DB,
                                        format!("[init_db()] flush_records() failed, error={}", e),
                                        None,
                                    );
                                }
                        }
                        FilterDbCmd::SaveRuleset { port, version, json, ts_ns } => {
                            if let Err(e) = save_ruleset_row(&mut conn, port, &version, &json, ts_ns) {
                                sink_ecam::record(
                                    Severity::Error,
                                    Origin::DB,
                                    format!("[init_db()] save_ruleset_row() failed, error={}", e),
                                    None,
                                );
                            }
                        }
                        FilterDbCmd::SaveConfig { external_port, upstream_port, json, ts_ns } => {
                            if let Err(e) = save_config_row(&mut conn, external_port, upstream_port, &json, ts_ns) {
                                sink_ecam::record(
                                    Severity::Error,
                                    Origin::DB,
                                    format!("[init_db()] save_config() failed, error={}", e),
                                    None,
                                );
                            }
                        }
                        FilterDbCmd::ReserveConnIds { reply } => {
                            let block = match reserve_conn_id_block(&mut conn) {
                                Ok(b) => b,
                                Err(e) => {
                                    sink_ecam::record(
                                        Severity::Error,
                                        Origin::DB,
                                        format!("[init_db()] reserve_conn_id_block() failed, error={}", e),
                                        None,
                                    );
                                    // Degraded: hand out a large in-process range so the
                                    // proxy keeps assigning (unique within this run).
                                    (1, u64::MAX)
                                }
                            };
                            let _ = reply.send(block);
                        }
                        FilterDbCmd::LoadRulesetHistory { port, reply } => {
                            let entries = load_ruleset_history(&conn, port).unwrap_or_else(|e| {
                                sink_ecam::record(
                                    Severity::Error,
                                    Origin::DB,
                                    format!("[init_db()] load_ruleset_history() failed, error={}", e),
                                    None,
                                );
                                Vec::new()
                            });
                            let _ = reply.send(entries);
                        }
                        FilterDbCmd::Shutdown => {
                            let _ = flush_records(&mut conn, &mut rec_buf);
                            info!("DB actor shutdown");
                            break;
                        }
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                    if !rec_buf.is_empty()
                        && let Err(e) = flush_records(&mut conn, &mut rec_buf) {
                            sink_ecam::record(
                                Severity::Error,
                                Origin::DB,
                                format!("[init_db()] periodic flush_records() failed, error={}", e),
                                None,
                            );
                        }
                }
            }
        }
    });

    Ok(DbHandle {
        tx,
        path: Arc::new(db_path_clone),
    })
}

fn flush_records(conn: &mut Connection, buf: &mut Vec<EvaluationRecord>) -> anyhow::Result<()> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO records (timestamp_ns, port, inbound, decision, json)
             VALUES (?,?,?,?,?)",
        )?;
        for rec in buf.drain(..) {
            let json = serde_json::to_string(&rec)?;
            let inbound = if rec.inbound { 1 } else { 0 };
            stmt.execute(params![
                rec.timestamp_ns as i64,
                rec.public_port as i64,
                inbound,
                rec.decision.as_str(),
                json,
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}
