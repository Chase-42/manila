use rusqlite::{Connection, Result};

// Each entry is (version_number, sql). Migrations run in order and are skipped
// if already recorded in schema_migrations. Each migration is atomic.
const MIGRATIONS: &[(i64, &str)] = &[(1, MIGRATION_001), (2, MIGRATION_002), (3, MIGRATION_003)];

const MIGRATION_001: &str = "
    CREATE TABLE accounts (
        id          TEXT NOT NULL PRIMARY KEY,
        name        TEXT NOT NULL,
        type        TEXT NOT NULL,
        subtype     TEXT NOT NULL,
        institution TEXT NOT NULL,
        currency    TEXT NOT NULL,
        created_at  TEXT NOT NULL
    );

    CREATE TABLE import_batches (
        id          TEXT NOT NULL PRIMARY KEY,
        source_type TEXT NOT NULL,
        account_id  TEXT REFERENCES accounts(id),
        filename    TEXT,
        imported_at TEXT NOT NULL
    );

    CREATE TABLE transactions (
        id         TEXT NOT NULL PRIMARY KEY,
        account_id TEXT NOT NULL REFERENCES accounts(id),
        created_at TEXT NOT NULL
    );

    CREATE TABLE raw_records (
        id              TEXT NOT NULL PRIMARY KEY,
        transaction_id  TEXT NOT NULL REFERENCES transactions(id),
        supersedes_id   TEXT REFERENCES raw_records(id),
        import_batch_id TEXT REFERENCES import_batches(id),
        source_id       TEXT NOT NULL,
        date            TEXT NOT NULL,
        amount_cents    INTEGER NOT NULL,
        description     TEXT NOT NULL,
        raw_json        TEXT NOT NULL,
        created_at      TEXT NOT NULL
    );

    CREATE TABLE postings (
        id             TEXT NOT NULL PRIMARY KEY,
        transaction_id TEXT NOT NULL REFERENCES transactions(id),
        account_id     TEXT NOT NULL REFERENCES accounts(id),
        amount_cents   INTEGER NOT NULL
    );
";

const MIGRATION_002: &str = "
    CREATE TABLE valuation_snapshots (
        id           TEXT    NOT NULL PRIMARY KEY,
        account_id   TEXT    NOT NULL REFERENCES accounts(id),
        date         TEXT    NOT NULL,
        amount_cents INTEGER NOT NULL,
        currency     TEXT    NOT NULL
    );
";

const MIGRATION_003: &str = "
    CREATE TABLE transaction_meta (
        transaction_id TEXT NOT NULL PRIMARY KEY REFERENCES transactions(id),
        notes          TEXT,
        tags           TEXT NOT NULL DEFAULT '[]',
        reviewed       INTEGER NOT NULL DEFAULT 0,
        updated_at     TEXT NOT NULL
    );

    CREATE TABLE categories (
        id         TEXT NOT NULL PRIMARY KEY,
        name       TEXT NOT NULL,
        kind       TEXT NOT NULL,
        created_at TEXT NOT NULL
    );

    CREATE TABLE category_assignments (
        id             TEXT NOT NULL PRIMARY KEY,
        transaction_id TEXT NOT NULL REFERENCES transactions(id),
        category_id    TEXT NOT NULL REFERENCES categories(id),
        amount_cents   INTEGER NOT NULL
    );
";

pub fn run_migrations(conn: &mut Connection) -> Result<()> {
    for &(version, sql) in MIGRATIONS {
        let already_applied: bool = conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = ?1",
            [version],
            |row| row.get::<_, i64>(0),
        )? > 0;

        if !already_applied {
            let tx = conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, datetime('now'))",
                [version],
            )?;
            tx.commit()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::open_connection;

    #[test]
    fn migration_001_creates_all_tables() {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();

        let tables: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .unwrap();
            stmt.query_map([], |row| row.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        };

        assert!(tables.contains(&"accounts".to_string()));
        assert!(tables.contains(&"import_batches".to_string()));
        assert!(tables.contains(&"postings".to_string()));
        assert!(tables.contains(&"raw_records".to_string()));
        assert!(tables.contains(&"transactions".to_string()));
    }

    #[test]
    fn migration_002_creates_valuation_snapshots() {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();

        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='valuation_snapshots'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
            > 0;

        assert!(exists);
    }

    #[test]
    fn migration_003_creates_overlay_and_budget_tables() {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();

        for table in &["transaction_meta", "categories", "category_assignments"] {
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap()
                > 0;
            assert!(exists, "table {table} should exist after migration 3");
        }
    }

    #[test]
    fn migrations_are_idempotent() {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();
        // second run must not error and must not insert a duplicate row
        run_migrations(&mut conn).unwrap();

        let version_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version_count, 1);
    }
}
