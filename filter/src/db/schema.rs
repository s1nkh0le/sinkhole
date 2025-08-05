use rusqlite::Connection;

pub fn init_schema(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        r#"

        "#,
    )?;
    Ok(())
}
