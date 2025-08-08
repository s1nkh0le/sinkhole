use rusqlite::Connection;

/// Executes schema initialization on the db
pub fn init_schema(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(include_str!("schema.sql"))?;
    Ok(())
}
