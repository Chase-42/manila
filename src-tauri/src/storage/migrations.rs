use rusqlite::{Connection, Result};

// Each entry is (version_number, sql). Migrations run in order and are skipped
// if already recorded in schema_migrations. Each migration is atomic.
const MIGRATIONS: &[(i64, &str)] = &[
    (1, MIGRATION_001),
    (2, MIGRATION_002),
    (3, MIGRATION_003),
    (4, MIGRATION_004),
    (5, MIGRATION_005),
    (6, MIGRATION_006),
    (7, MIGRATION_007),
    (8, MIGRATION_008),
    (9, MIGRATION_009),
    (10, MIGRATION_010),
    (11, MIGRATION_011),
    (12, MIGRATION_012),
    (13, MIGRATION_013),
];

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

const MIGRATION_004: &str = "
    CREATE TABLE allocation_events (
        id                       TEXT NOT NULL PRIMARY KEY,
        category_id              TEXT NOT NULL REFERENCES categories(id),
        month                    TEXT NOT NULL,
        amount_cents             INTEGER NOT NULL,
        kind                     TEXT NOT NULL,
        counterpart_category_id  TEXT REFERENCES categories(id),
        group_id                 TEXT,
        note                     TEXT,
        created_at               TEXT NOT NULL
    );

    CREATE TABLE monthly_targets (
        month        TEXT NOT NULL PRIMARY KEY,
        amount_cents INTEGER NOT NULL
    );
";

const MIGRATION_005: &str = "
    CREATE TABLE category_groups (
        id         TEXT    NOT NULL PRIMARY KEY,
        name       TEXT    NOT NULL,
        sort_order INTEGER NOT NULL,
        created_at TEXT    NOT NULL
    );

    ALTER TABLE categories ADD COLUMN group_id TEXT REFERENCES category_groups(id);
";

const MIGRATION_006: &str = "
    CREATE TABLE income_categories (
        id         TEXT    NOT NULL PRIMARY KEY,
        name       TEXT    NOT NULL,
        hidden     INTEGER NOT NULL DEFAULT 0,
        created_at TEXT    NOT NULL
    );

    CREATE TABLE splits (
        id             TEXT    NOT NULL PRIMARY KEY,
        transaction_id TEXT    NOT NULL REFERENCES transactions(id),
        target_type    TEXT    NOT NULL CHECK(target_type IN ('envelope','income')),
        target_id      TEXT    NOT NULL,
        amount_cents   INTEGER NOT NULL
    );

    INSERT INTO splits (id, transaction_id, target_type, target_id, amount_cents)
    SELECT id, transaction_id, 'envelope', category_id, amount_cents
    FROM category_assignments;

    DROP TABLE category_assignments;

    DROP TABLE monthly_targets;
";

const MIGRATION_007: &str = "
    CREATE TABLE allocation_events_new (
        id                       TEXT NOT NULL PRIMARY KEY,
        category_id              TEXT NOT NULL REFERENCES categories(id),
        month                    TEXT NOT NULL,
        amount_cents             INTEGER NOT NULL,
        kind                     TEXT NOT NULL,
        counterpart_category_id  TEXT REFERENCES categories(id),
        note                     TEXT,
        created_at               TEXT NOT NULL
    );

    INSERT INTO allocation_events_new
        (id, category_id, month, amount_cents, kind, counterpart_category_id, note, created_at)
    SELECT id, category_id, month, amount_cents, kind, counterpart_category_id, note, created_at
    FROM allocation_events;

    DROP TABLE allocation_events;

    ALTER TABLE allocation_events_new RENAME TO allocation_events;
";

const MIGRATION_008: &str = "
    CREATE TABLE month_closes (
        month      TEXT NOT NULL PRIMARY KEY,
        closed_at  TEXT NOT NULL
    );
";

const MIGRATION_010: &str = "
    CREATE TABLE categorization_rules (
        id               TEXT    NOT NULL PRIMARY KEY,
        merchant_pattern TEXT    NOT NULL UNIQUE,
        category_id      TEXT    NOT NULL REFERENCES categories(id),
        priority         INTEGER NOT NULL DEFAULT 0,
        created_at       TEXT    NOT NULL
    );
";

const MIGRATION_011: &str = "
    CREATE TABLE goals (
        id                  TEXT    NOT NULL PRIMARY KEY,
        name                TEXT    NOT NULL,
        target_amount_cents INTEGER NOT NULL CHECK(target_amount_cents > 0),
        category_id         TEXT    REFERENCES categories(id) ON DELETE SET NULL,
        target_date         TEXT,
        achieved_at         TEXT,
        created_at          TEXT    NOT NULL
    );
";

const MIGRATION_012: &str = "
    CREATE TABLE vault_config (
        salt                     BLOB NOT NULL,
        encrypted_vault_secret   BLOB NOT NULL,
        created_at               TEXT NOT NULL
    );
";

const MIGRATION_013: &str = "
    ALTER TABLE vault_config ADD COLUMN phrase_verifier BLOB;
";

const MIGRATION_009: &str = "
    CREATE VIRTUAL TABLE transactions_fts USING fts5(
        description,
        notes,
        transaction_id UNINDEXED
    );

    INSERT INTO transactions_fts (rowid, description, notes, transaction_id)
    SELECT rr.rowid, rr.description, COALESCE(tm.notes, ''), rr.transaction_id
    FROM raw_records rr
    LEFT JOIN transaction_meta tm ON tm.transaction_id = rr.transaction_id
    WHERE NOT EXISTS (
        SELECT 1 FROM raw_records rr2 WHERE rr2.supersedes_id = rr.id
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
    fn migration_003_creates_overlay_tables() {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();

        for table in &["transaction_meta", "categories"] {
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
    fn migration_004_creates_allocation_events() {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();

        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='allocation_events'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
            > 0;
        assert!(
            exists,
            "allocation_events table should exist after migration 4"
        );
    }

    #[test]
    fn migration_005_creates_category_groups_and_group_id_column() {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();

        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='category_groups'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
            > 0;
        assert!(
            table_exists,
            "category_groups table should exist after migration 5"
        );

        let column_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('categories') WHERE name='group_id'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
            > 0;
        assert!(
            column_exists,
            "categories.group_id column should exist after migration 5"
        );
    }

    #[test]
    fn migration_006_creates_income_categories_and_splits() {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();

        for table in &["income_categories", "splits"] {
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap()
                > 0;
            assert!(exists, "table {table} should exist after migration 6");
        }

        for dropped in &["category_assignments", "monthly_targets"] {
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [dropped],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap()
                > 0;
            assert!(
                !exists,
                "table {dropped} should not exist after migration 6"
            );
        }
    }

    #[test]
    fn migration_007_drops_group_id_from_allocation_events() {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();

        let has_group_id: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('allocation_events') WHERE name='group_id'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
            > 0;
        assert!(
            !has_group_id,
            "allocation_events.group_id should not exist after migration 7"
        );

        // Core columns still present
        for col in &[
            "id",
            "category_id",
            "month",
            "amount_cents",
            "kind",
            "note",
            "created_at",
        ] {
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info('allocation_events') WHERE name=?1",
                    [col],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap()
                > 0;
            assert!(
                exists,
                "allocation_events.{col} should still exist after migration 7"
            );
        }
    }

    #[test]
    fn migration_008_creates_month_closes() {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();

        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='month_closes'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
            > 0;
        assert!(exists, "month_closes table should exist after migration 8");
    }

    #[test]
    fn migration_009_creates_transactions_fts() {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();

        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='transactions_fts'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
            > 0;
        assert!(
            exists,
            "transactions_fts virtual table should exist after migration 9"
        );
    }

    #[test]
    fn migration_010_creates_categorization_rules() {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();

        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='categorization_rules'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
            > 0;
        assert!(
            exists,
            "categorization_rules table should exist after migration 10"
        );

        // UNIQUE constraint: inserting two rows with the same merchant_pattern must fail.
        conn.execute(
            "INSERT INTO categories (id, name, kind, created_at) VALUES ('cat-1', 'Test', 'flow', datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO categorization_rules (id, merchant_pattern, category_id, priority, created_at) VALUES ('r1', 'Grocery', 'cat-1', 0, datetime('now'))",
            [],
        )
        .unwrap();
        let dup_result = conn.execute(
            "INSERT INTO categorization_rules (id, merchant_pattern, category_id, priority, created_at) VALUES ('r2', 'Grocery', 'cat-1', 0, datetime('now'))",
            [],
        );
        assert!(
            dup_result.is_err(),
            "inserting a duplicate merchant_pattern should fail due to UNIQUE constraint"
        );

        for col in &[
            "id",
            "merchant_pattern",
            "category_id",
            "priority",
            "created_at",
        ] {
            let col_exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info('categorization_rules') WHERE name=?1",
                    [col],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap()
                > 0;
            assert!(
                col_exists,
                "categorization_rules.{col} should exist after migration 10"
            );
        }
    }

    #[test]
    fn migration_011_creates_goals() {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();

        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='goals'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
            > 0;
        assert!(exists, "goals table should exist after migration 11");

        for col in &[
            "id",
            "name",
            "target_amount_cents",
            "category_id",
            "target_date",
            "achieved_at",
            "created_at",
        ] {
            let col_exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info('goals') WHERE name=?1",
                    [col],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap()
                > 0;
            assert!(col_exists, "goals.{col} should exist after migration 11");
        }
    }

    #[test]
    fn migration_012_creates_vault_config() {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();

        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='vault_config'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
            > 0;
        assert!(
            table_exists,
            "vault_config table should exist after migration 12"
        );

        for col in &["salt", "encrypted_vault_secret", "created_at"] {
            let col_exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info('vault_config') WHERE name=?1",
                    [col],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap()
                > 0;
            assert!(
                col_exists,
                "vault_config.{col} should exist after migration 12"
            );
        }
    }

    #[test]
    fn migration_013_adds_phrase_verifier_column() {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();

        let col_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('vault_config') WHERE name='phrase_verifier'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
            > 0;
        assert!(
            col_exists,
            "vault_config.phrase_verifier should exist after migration 13"
        );
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
