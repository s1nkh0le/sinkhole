use std::sync::Arc;
use tokio::sync::mpsc;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde_json::json;
use tracing::{error, info};

use crate::observability::eval_record::EvaluationRecord;
use crate::rules::RuleSetRaw;
use crate::types::Scope;

#[derive(Clone)]
pub struct DbHandle {
    pub(crate) tx: mpsc::Sender<DbCmd>,
}

pub enum DbCmd {
    InsertRecord(EvaluationRecord),
    SaveRuleset { port: u16, version: String, json: String, ts_ns: i64 },
    SaveConfig  { port: u16, json: String, ts_ns: i64 },
    Shutdown,
}

pub struct HydratedPortData {
    pub ruleset: Option<(RuleSetRaw, String)>, // (raw, version)
    pub config:  Option<HydratedConfig>,
}

pub struct HydratedConfig {
    pub name: Option<String>,
    pub comment: Option<String>,
    pub hex_color: Option<String>,
    pub enabled_scopes: Vec<Scope>,
}

pub fn init_db(path: &str) -> Result<DbHandle> {

    {
        let conn = Connection::open(path)?;
        init_schema(&conn)?;
    }

    // spawn actor
    let (tx, mut rx) = mpsc::channel::<DbCmd>(20_000);
    let db_path = path.to_string();

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
                                    error!(?e, "flush_records failed");
                                }
                            }
                        }
                        DbCmd::SaveRuleset { port, version, json, ts_ns } => {
                            if let Err(e) = save_ruleset_row(&mut conn, port, &version, &json, ts_ns) {
                                error!(?e, port, "save_ruleset failed");
                            }
                        }
                        DbCmd::SaveConfig { port, json, ts_ns } => {
                            if let Err(e) = save_config_row(&mut conn, port, &json, ts_ns) {
                                error!(?e, port, "save_config failed");
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
                            error!(?e, "periodic flush failed");
                        }
                    }
                }
            }
        }
    });

    Ok(DbHandle { tx })
}

fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;

        CREATE TABLE IF NOT EXISTS records(
          id            INTEGER PRIMARY KEY AUTOINCREMENT,
          timestamp_ns  INTEGER NOT NULL,
          port          INTEGER NOT NULL,
          inbound       INTEGER NOT NULL,
          decision      TEXT NOT NULL,
          json          TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_records_time_port ON records(timestamp_ns, port);

        CREATE TABLE IF NOT EXISTS port_rulesets(
          port          INTEGER PRIMARY KEY,
          version       TEXT NOT NULL,
          json          TEXT NOT NULL,
          updated_at_ns INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS port_configs(
          port          INTEGER PRIMARY KEY,
          name          TEXT,
          comment       TEXT,
          hex_color     TEXT,
          enabled_scopes_json TEXT NOT NULL,
          updated_at_ns INTEGER NOT NULL
        );
        "#
    )?;
    Ok(())
}

fn flush_records(conn: &mut Connection, buf: &mut Vec<EvaluationRecord>) -> Result<()> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO records (timestamp_ns, port, inbound, decision, json)
             VALUES (?,?,?,?,?)"
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

fn save_ruleset_row(conn: &mut Connection, port: u16, version: &str, json: &str, ts_ns: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO port_rulesets (port, version, json, updated_at_ns)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(port) DO UPDATE SET version=excluded.version, json=excluded.json, updated_at_ns=excluded.updated_at_ns",
        params![port as i64, version, json, ts_ns]
    )?;
    Ok(())
}

fn save_config_row(conn: &mut Connection, port: u16, json: &str, ts_ns: i64) -> Result<()> {
    // Extract small fields for convenience? We can defer; json has all.
    let v: serde_json::Value = serde_json::from_str(json)?;
    let name      = v.get("name").and_then(|x| x.as_str());
    let comment   = v.get("comment").and_then(|x| x.as_str());
    let hex_color = v.get("hex_color").and_then(|x| x.as_str());
    let scopes_j  = v.get("enabled_scopes").unwrap_or(&serde_json::Value::Null).to_string();

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

// ---------- Hydration reads (blocking, call at startup) -----------------
pub fn hydrate_registry(db_path: &str, registry: &crate::registry::PortRegistry) -> Result<()> {
    let ports: Vec<u16> = registry.iter().map(|e| e.binding.public).collect();
    let data = hydrate_for_ports(db_path, &ports)?;
    for (port, h) in data {
        if let Some(pr) = registry.get(&port) {
            if let Some(cfgp) = h.config {
                if let Some(n) = cfgp.name { pr.name.store(Some(Arc::new(n))); }
                if let Some(c) = cfgp.comment { pr.comment.store(Some(Arc::new(c))); }
                if let Some(hc) = cfgp.hex_color { pr.hex_color.store(Some(Arc::new(hc))); }
                let mut g = pr.enabled_scopes.write().unwrap();
                g.clear();
                g.extend(cfgp.enabled_scopes.into_iter());
            }
            if let Some((raw, _ver)) = h.ruleset {
                let compiled = crate::rules::compile::compile_ruleset(&raw)?;
                pr.raw_ruleset.store(Arc::new(raw));
                pr.compiled.store(Arc::new(compiled));
            }
        }
    }
    Ok(())
}

pub fn hydrate_for_ports(path: &str, ports: &[u16]) -> Result<Vec<(u16, HydratedPortData)>> {
    let mut conn = Connection::open(path)?;
    // ensure tables exist (idempotent)
    init_schema(&conn)?;
    let mut out = Vec::new();
    for p in ports {
        let ruleset = load_ruleset(&mut conn, *p)?;
        let config  = load_config(&mut conn, *p)?;
        out.push((*p, HydratedPortData { ruleset, config }));
    }
    Ok(out)
}


fn load_ruleset(conn: &mut Connection, port: u16) -> Result<Option<(RuleSetRaw, String)>> {
    let mut stmt = conn.prepare(
        "SELECT version, json FROM port_rulesets WHERE port = ?"
    )?;
    let mut rows = stmt.query(params![port as i64])?;
    if let Some(row) = rows.next()? {
        let version: String = row.get(0)?;
        let json_str: String = row.get(1)?;
        let raw: RuleSetRaw = serde_json::from_str(&json_str)?;
        Ok(Some((raw, version)))
    } else {
        Ok(None)
    }
}

fn load_config(conn: &mut Connection, port: u16) -> Result<Option<HydratedConfig>> {
    let mut stmt = conn.prepare(
        "SELECT name, comment, hex_color, enabled_scopes_json
         FROM port_configs WHERE port = ?"
    )?;
    let mut rows = stmt.query(params![port as i64])?;
    if let Some(row) = rows.next()? {
        let name: Option<String> = row.get(0)?;
        let comment: Option<String> = row.get(1)?;
        let hex_color: Option<String> = row.get(2)?;
        let scopes_json: String = row.get(3)?;
        let scopes: Vec<Scope> = serde_json::from_str(&scopes_json).unwrap_or_default();
        Ok(Some(HydratedConfig { name, comment, hex_color, enabled_scopes: scopes }))
    } else {
        Ok(None)
    }
}

