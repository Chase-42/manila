use rusqlite::{Connection, Result};

/// Opens a SQLite connection at `path` (use `:memory:` in tests).
/// FK enforcement is set at every connection open, not once at schema creation.
pub fn open_connection(path: &str) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    bootstrap_migrations_table(&conn)?;
    Ok(conn)
}

fn bootstrap_migrations_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version    INTEGER PRIMARY KEY,
            applied_at TEXT    NOT NULL
        );",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_migrations_exists_after_open() {
        let conn = open_connection(":memory:").unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='schema_migrations'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}
