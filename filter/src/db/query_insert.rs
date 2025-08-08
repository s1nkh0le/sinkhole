use rusqlite::{params, Connection};

pub fn save_ruleset_row(
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

pub fn save_config_row(conn: &mut Connection, external_port: u16, upstream_port: u16, json: &str, ts_ns: i64) -> anyhow::Result<()> {
    // Extract small fields for convenience
    let v: serde_json::Value = serde_json::from_str(json)?;

    let name = v.get("name").and_then(|x| x.as_str());
    let comment = v.get("comment").and_then(|x| x.as_str());
    let hex_color = v.get("hex_color").and_then(|x| x.as_str());
    let scopes_j = v.get("enabled_scopes").unwrap_or(&serde_json::Value::Null).to_string();

    conn.execute(
        "INSERT INTO port_configs (external_port, upstream_port, name, comment, hex_color, enabled_scopes_json, updated_at_ns)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(external_port) DO UPDATE SET
           name=excluded.name,
           comment=excluded.comment,
           hex_color=excluded.hex_color,
           enabled_scopes_json=excluded.enabled_scopes_json,
           updated_at_ns=excluded.updated_at_ns",
        params![
            external_port as i64,
            upstream_port as i64,
            name,
            comment,
            hex_color,
            scopes_j,
            ts_ns
        ]
    )?;
    Ok(())
}
