use std::sync::Arc;
use rusqlite::{params, Connection};
use serde_json::json;

pub async fn fetch_records_since(
    path: Arc<String>,
    port: u16,
    since_ns: i64,
    limit: usize,
) -> anyhow::Result<(Vec<serde_json::Value>, i64)> {
    tokio::task::spawn_blocking(move || {
        let conn = Connection::open(&*path)?;
        let mut stmt = conn.prepare(
            "SELECT timestamp_ns, json
             FROM records
             WHERE port = ? AND timestamp_ns > ?
             ORDER BY timestamp_ns
             LIMIT ?"
        )?;
        let mut rows = stmt.query(params![port as i64, since_ns, limit as i64])?;

        let mut out = Vec::with_capacity(limit);
        let mut last_ts = since_ns;
        while let Some(row) = rows.next()? {
            let ts: i64 = row.get(0)?;
            let js: String = row.get(1)?;
            let v: serde_json::Value = serde_json::from_str(&js)?;
            out.push(v);
            last_ts = ts;
        }
        Ok::<_, anyhow::Error>((out, last_ts))
    })
        .await?
}

pub async fn fetch_records_filtered(
    path: Arc<String>,
    port: u16,
    since: i64,
    until: i64,
    limit: usize,
    asc: bool,
    decision: Option<&str>,
    dir: Option<&str>,
    reason: Option<&str>,
) -> anyhow::Result<(Vec<serde_json::Value>, i64)> {
    // Own the strings for 'static closure
    let decision = decision.map(ToOwned::to_owned);
    let dir      = dir.map(ToOwned::to_owned);
    let reason   = reason.map(ToOwned::to_owned);

    tokio::task::spawn_blocking(move || {
        use rusqlite::ToSql;

        let conn = Connection::open(&*path)?;

        // Build SQL ------------------------------------------------------
        let mut sql = String::from(
            "SELECT timestamp_ns, json
             FROM records
             WHERE port = ? AND timestamp_ns > ? AND timestamp_ns <= ?"
        );

        // Keep a parallel dbg list just for logging
        let mut dbg_params: Vec<String> = Vec::new();
        let mut params: Vec<Box<dyn ToSql>> = Vec::new();

        params.push(Box::new(port as i64));
        dbg_params.push(format!("{}", port));

        params.push(Box::new(since));
        dbg_params.push(format!("{}", since));

        params.push(Box::new(until));
        dbg_params.push(format!("{}", until));

        if let Some(ref d) = decision {
            sql.push_str(" AND decision = ?");
            params.push(Box::new(d.clone()));
            dbg_params.push(d.clone());
        }

        if let Some(ref d) = dir {
            match d.as_str() {
                "in"  => {
                    sql.push_str(" AND inbound = 1");
                    dbg_params.push("<inbound=1>".into());
                }
                "out" => {
                    sql.push_str(" AND inbound = 0");
                    dbg_params.push("<inbound=0>".into());
                }
                _ => {}
            }
        }

        if let Some(ref r) = reason {
            sql.push_str(
                " AND EXISTS (
                    SELECT 1
                    FROM json_each(records.json, '$.drop_reasons')
                    WHERE json_extract(json_each.value, '$.kind') = ?
                  )"
            );
            params.push(Box::new(r.clone()));
            dbg_params.push(r.clone());
        }

        sql.push_str(if asc { " ORDER BY timestamp_ns ASC" } else { " ORDER BY timestamp_ns DESC" });
        sql.push_str(" LIMIT ? ");
        params.push(Box::new(limit as i64));
        dbg_params.push(limit.to_string());

        // Log final query + params
        tracing::debug!(
            target: "db.logs",
            sql = %sql,
            params = ?dbg_params,
            "executing /logs query"
        );

        // Execute --------------------------------------------------------
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(params.iter().map(|b| &**b)))?;

        let mut out = Vec::with_capacity(limit);
        let mut last_ts = since;
        while let Some(row) = rows.next()? {
            let ts: i64 = row.get(0)?;
            let js: String = row.get(1)?;
            out.push(serde_json::from_str(&js)?);
            last_ts = ts;
        }

        tracing::debug!(
            target: "db.logs",
            rows = out.len(),
            last_ts,
            "logs query result"
        );

        Ok::<_, anyhow::Error>((out, last_ts))
    })
        .await?
}


pub async fn db_health(path: Arc<String>) -> anyhow::Result<String> {
    tokio::task::spawn_blocking(move || {
        let conn = Connection::open(&*path)?;
        let mut stmt = conn.prepare("PRAGMA quick_check")?;
        let mut rows = stmt.query([])?;
        let mut res = String::new();
        while let Some(r) = rows.next()? {
            let s: String = r.get(0)?;
            if !res.is_empty() { res.push_str(", "); }
            res.push_str(&s);
        }
        Ok(res)
    }).await?
}

pub async fn db_stats(path: Arc<String>) -> anyhow::Result<serde_json::Value> {
    tokio::task::spawn_blocking(move || {
        let conn = Connection::open(&*path)?;

        let row_count: i64 = conn.query_row("SELECT COUNT(*) FROM records", [], |r| r.get(0))?;
        let first_ts: Option<i64> = conn.query_row("SELECT MIN(timestamp_ns) FROM records", [], |r| r.get(0)).ok();
        let last_ts: Option<i64>  = conn.query_row("SELECT MAX(timestamp_ns) FROM records", [], |r| r.get(0)).ok();

        let page_count: i64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let page_size:  i64 = conn.query_row("PRAGMA page_size",  [], |r| r.get(0))?;
        let file_bytes = page_count * page_size;

        Ok(json!({
            "rows": row_count,
            "first_ts": first_ts,
            "last_ts": last_ts,
            "file_bytes": file_bytes
        }))
    }).await?
}


pub async fn clear_db(path: Arc<String>) -> anyhow::Result<()> {
    tokio::task::spawn_blocking(move || {
        let conn = Connection::open(&*path)?;
        conn.execute_batch(
            "DELETE FROM records;
             DELETE FROM port_rulesets;
             DELETE FROM port_configs;
             VACUUM;"
        )?;
        Ok::<_, anyhow::Error>(())
    })
        .await?
}
