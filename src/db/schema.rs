use rusqlite::Connection;

pub fn init_schema(conn: &Connection) -> anyhow::Result<()> {
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