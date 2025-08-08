use crate::db::schema::init_schema;
use crate::observability::sinks::sink_ecam;
use crate::observability::sinks::sink_ecam::{Origin, Severity};
use rusqlite::{Connection, params};
use shared::types::record_types::EvaluationRecord;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info};
use crate::db::query_insert::{save_config_row, save_ruleset_row};

#[derive(Clone)]
pub struct DbHandle {
    pub(crate) tx: mpsc::Sender<DbCmd>,
    pub path: Arc<String>,
}

pub enum DbCmd {
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
    Shutdown,
}

pub fn init_db(path: &str) -> anyhow::Result<DbHandle> {
    {
        // pre-init schema
        let conn = Connection::open(path)?;
        init_schema(&conn)?;
    }

    // spawn actor
    let (tx, mut rx) = mpsc::channel::<DbCmd>(20_000);
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
                        DbCmd::InsertRecord(rec) => {
                            rec_buf.push(rec);
                            if rec_buf.len() >= 512 {
                                if let Err(e) = flush_records(&mut conn, &mut rec_buf) {
                                    sink_ecam::record(
                                        Severity::Error,
                                        Origin::DB,
                                        format!("[init_db()] flush_records() failed, error={}", e),
                                        None,
                                    );
                                }
                            }
                        }
                        DbCmd::SaveRuleset { port, version, json, ts_ns } => {
                            if let Err(e) = save_ruleset_row(&mut conn, port, &version, &json, ts_ns) {
                                sink_ecam::record(
                                    Severity::Error,
                                    Origin::DB,
                                    format!("[init_db()] save_ruleset_row() failed, error={}", e),
                                    None,
                                );
                            }
                        }
                        DbCmd::SaveConfig { external_port, upstream_port, json, ts_ns } => {
                            if let Err(e) = save_config_row(&mut conn, external_port, upstream_port, &json, ts_ns) {
                                sink_ecam::record(
                                    Severity::Error,
                                    Origin::DB,
                                    format!("[init_db()] save_config() failed, error={}", e),
                                    None,
                                );
                            }
                        }
                        DbCmd::Shutdown => {
                            let _ = flush_records(&mut conn, &mut rec_buf);
                            info!("DB actor shutdown");
                            break;
                        }
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                    if !rec_buf.is_empty() {
                        if let Err(e) = flush_records(&mut conn, &mut rec_buf) {
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