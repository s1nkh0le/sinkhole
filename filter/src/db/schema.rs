use rusqlite::Connection;

/// Executes schema initialization on the db (idempotent `CREATE TABLE IF NOT
/// EXISTS` statements). No migrations: in beta we just delete the DB and restart.
pub fn init_schema(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(include_str!("schema.sql"))?;
    Ok(())
}
