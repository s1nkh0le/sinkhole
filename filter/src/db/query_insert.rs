use rusqlite::{Connection, OptionalExtension, params};

/// Reservation granularity for the monotonic connection-id allocator. Each
/// reserved block is exactly one "decade" wide and aligned to a multiple of it,
/// so every filter run begins from a clean 10,000-aligned conn_id (nice to read)
/// and the next run resumes strictly above the previous reservation — ids are
/// never reused, even across restarts and aggressive pruning.
pub const CONN_ID_BLOCK: u64 = 10_000;

/// Atomically reserve the next block of connection ids: returns `(base, ceiling)`
/// where ids `base..ceiling` are now exclusively this run's to hand out. The
/// reserved ceiling is persisted in `meta` *before* returning, so a crash can
/// never cause a later boot to reuse ids from this block (it always continues
/// above the stored ceiling). Unused ids in a block are simply skipped (gaps).
pub fn reserve_conn_id_block(conn: &mut Connection) -> anyhow::Result<(u64, u64)> {
    let tx = conn.transaction()?;
    let stored: i64 = tx
        .query_row(
            "SELECT value FROM meta WHERE key = 'conn_id_ceiling'",
            [],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0);

    // `stored` is always a multiple of CONN_ID_BLOCK (we only ever write such
    // values), so the base is already clean. First ever run starts at one block in
    // (so we begin at 10_000, not 0).
    let base = if stored <= 0 {
        CONN_ID_BLOCK
    } else {
        stored as u64
    };
    let ceiling = base + CONN_ID_BLOCK;

    tx.execute(
        "INSERT INTO meta (key, value) VALUES ('conn_id_ceiling', ?1)
         ON CONFLICT(key) DO UPDATE SET value = ?1",
        params![ceiling as i64],
    )?;
    tx.commit()?;
    Ok((base, ceiling))
}

pub fn save_ruleset_row(
    conn: &mut Connection,
    port: u16,
    version: &str,
    json: &str,
    ts_ns: i64,
) -> anyhow::Result<()> {
    // Append a new history row; the latest (highest id) is the active ruleset.
    conn.execute(
        "INSERT INTO port_rulesets (external_port, version, json, updated_at_ns)
         VALUES (?, ?, ?, ?)",
        params![port as i64, version, json, ts_ns],
    )?;
    Ok(())
}

pub fn save_config_row(
    conn: &mut Connection,
    external_port: u16,
    upstream_port: u16,
    json: &str,
    ts_ns: i64,
) -> anyhow::Result<()> {
    // Extract small fields for convenience
    let v: serde_json::Value = serde_json::from_str(json)?;

    let name = v.get("name").and_then(|x| x.as_str());
    let comment = v.get("comment").and_then(|x| x.as_str());
    let hex_color = v.get("hex_color").and_then(|x| x.as_str());
    let scopes_j = v
        .get("enabled_scopes")
        .unwrap_or(&serde_json::Value::Null)
        .to_string();

    // Append a new history row; the latest (highest id) is the active config.
    conn.execute(
        "INSERT INTO port_configs (external_port, upstream_port, name, comment, hex_color, enabled_scopes_json, updated_at_ns)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
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
