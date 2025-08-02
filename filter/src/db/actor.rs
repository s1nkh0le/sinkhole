use crate::db::schema::init_schema;
use crate::observability::sinks::sink_ecam;
use crate::observability::sinks::sink_ecam::{Origin, Severity};
use rusqlite::{Connection, params};
use shared::eval_record::EvaluationRecord;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info};

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
        port: u16,
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
                        DbCmd::SaveConfig { port, json, ts_ns } => {
                            if let Err(e) = save_config_row(&mut conn, port, &json, ts_ns) {
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

fn save_ruleset_row(
    conn: &mut Connection,
    port: u16,
    version: &str,
    json: &str,
    ts_ns: i64,
) -> anyhow::Result<()> {
    conn.execute(
        "INSERT INTO port_rulesets (port, version, json, updated_at_ns)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(port) DO UPDATE SET version=excluded.version, json=excluded.json, updated_at_ns=excluded.updated_at_ns",
        params![port as i64, version, json, ts_ns]
    )?;
    Ok(())
}

fn save_config_row(conn: &mut Connection, port: u16, json: &str, ts_ns: i64) -> anyhow::Result<()> {
    // Extract small fields for convenience? We can defer; json has all.
    let v: serde_json::Value = serde_json::from_str(json)?;
    let name = v.get("name").and_then(|x| x.as_str());
    let comment = v.get("comment").and_then(|x| x.as_str());
    let hex_color = v.get("hex_color").and_then(|x| x.as_str());
    let scopes_j = v
        .get("enabled_scopes")
        .unwrap_or(&serde_json::Value::Null)
        .to_string();

    conn.execute(
        "INSERT INTO port_configs (port, name, comment, hex_color, enabled_scopes_json, updated_at_ns)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(port) DO UPDATE SET
           name=excluded.name,
           comment=excluded.comment,
           hex_color=excluded.hex_color,
           enabled_scopes_json=excluded.enabled_scopes_json,
           updated_at_ns=excluded.updated_at_ns",
        params![
            port as i64,
            name,
            comment,
            hex_color,
            scopes_j,
            ts_ns
        ]
    )?;
    Ok(())
}
