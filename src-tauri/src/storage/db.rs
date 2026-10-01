use rusqlite::{Connection, Result};

/// Opens a SQLite/SQLCipher connection at `path` (use `:memory:` in tests).
/// When `key` is provided the connection is encrypted via PRAGMA key using the
/// raw 32-byte value (not SQLCipher's passphrase KDF).
/// FK enforcement is set at every connection open.
pub fn open_connection(path: &str, key: Option<&[u8; 32]>) -> Result<Connection> {
    let conn = Connection::open(path)?;
    if let Some(k) = key {
        let hex: String = k.iter().map(|b| format!("{b:02x}")).collect();
        conn.execute_batch(&format!("PRAGMA key = \"x'{hex}'\";"))?;
    }
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
    use tempfile::TempDir;

    #[test]
    fn schema_migrations_exists_after_open() {
        let conn = open_connection(":memory:", None).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='schema_migrations'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn keyed_connection_round_trips_data() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let path_str = path.to_str().unwrap();
        let key = [0x42u8; 32];

        {
            let conn = open_connection(path_str, Some(&key)).unwrap();
            conn.execute_batch("CREATE TABLE t (v TEXT NOT NULL); INSERT INTO t VALUES ('hello');")
                .unwrap();
        }

        let conn2 = open_connection(path_str, Some(&key)).unwrap();
        let val: String = conn2
            .query_row("SELECT v FROM t", [], |row| row.get(0))
            .unwrap();
        assert_eq!(val, "hello");
    }

    #[test]
    fn wrong_key_cannot_open_encrypted_db() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("locked.db");
        let path_str = path.to_str().unwrap();
        let key = [0x11u8; 32];
        let wrong_key = [0x22u8; 32];

        {
            let conn = open_connection(path_str, Some(&key)).unwrap();
            conn.execute_batch("CREATE TABLE t (v TEXT NOT NULL);")
                .unwrap();
        }

        // bootstrap_migrations_table runs inside open_connection; it fails with the wrong key.
        let result = open_connection(path_str, Some(&wrong_key));
        assert!(result.is_err(), "wrong key should fail to open");
    }
}
