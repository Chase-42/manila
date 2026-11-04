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
    // 12 and 13 created vault_config; that table is now vault.json on disk
    (15, MIGRATION_015),
    (16, MIGRATION_016),
    (17, MIGRATION_017),
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

const MIGRATION_015: &str = "
    CREATE TABLE plaid_connections (
        id               TEXT NOT NULL PRIMARY KEY,
        plaid_item_id    TEXT NOT NULL UNIQUE,
        access_token     TEXT NOT NULL,
        institution_name TEXT NOT NULL,
        institution_id   TEXT NOT NULL,
        account_ids      TEXT NOT NULL,
        connected_at     TEXT NOT NULL,
        last_synced_at   TEXT
    );
";

const MIGRATION_016: &str = "
    ALTER TABLE accounts ADD COLUMN plaid_account_id TEXT;
    CREATE UNIQUE INDEX idx_accounts_plaid_account_id
        ON accounts(plaid_account_id) WHERE plaid_account_id IS NOT NULL;
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

const MIGRATION_017: &str = "
    CREATE TABLE sync_state (
        key   TEXT NOT NULL PRIMARY KEY,
        value TEXT NOT NULL
    );

    CREATE TABLE sync_pending_changes (
        id         INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
        table_name TEXT    NOT NULL,
        row_id     TEXT    NOT NULL,
        operation  TEXT    NOT NULL CHECK(operation IN ('upsert', 'delete')),
        created_at TEXT    NOT NULL
    );

    CREATE TABLE sync_vector_clock (
        table_name TEXT    NOT NULL PRIMARY KEY,
        local_seq  INTEGER NOT NULL DEFAULT 0
    );

    INSERT INTO sync_vector_clock (table_name, local_seq) VALUES
        ('accounts', 0), ('transactions', 0), ('raw_records', 0),
        ('import_batches', 0), ('postings', 0), ('transaction_meta', 0),
        ('splits', 0), ('categories', 0), ('category_groups', 0),
        ('income_categories', 0), ('allocation_events', 0), ('month_closes', 0),
        ('goals', 0), ('valuation_snapshots', 0), ('categorization_rules', 0),
        ('plaid_connections', 0);

    CREATE TRIGGER sync_capture_accounts_insert AFTER INSERT ON accounts BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'accounts', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'accounts'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_accounts_update AFTER UPDATE ON accounts BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'accounts', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'accounts'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_accounts_delete AFTER DELETE ON accounts BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'accounts', OLD.id, 'delete', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'accounts'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;

    CREATE TRIGGER sync_capture_transactions_insert AFTER INSERT ON transactions BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'transactions', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'transactions'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_transactions_update AFTER UPDATE ON transactions BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'transactions', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'transactions'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_transactions_delete AFTER DELETE ON transactions BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'transactions', OLD.id, 'delete', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'transactions'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;

    CREATE TRIGGER sync_capture_raw_records_insert AFTER INSERT ON raw_records BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'raw_records', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'raw_records'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_raw_records_update AFTER UPDATE ON raw_records BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'raw_records', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'raw_records'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_raw_records_delete AFTER DELETE ON raw_records BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'raw_records', OLD.id, 'delete', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'raw_records'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;

    CREATE TRIGGER sync_capture_import_batches_insert AFTER INSERT ON import_batches BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'import_batches', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'import_batches'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_import_batches_update AFTER UPDATE ON import_batches BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'import_batches', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'import_batches'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_import_batches_delete AFTER DELETE ON import_batches BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'import_batches', OLD.id, 'delete', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'import_batches'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;

    CREATE TRIGGER sync_capture_postings_insert AFTER INSERT ON postings BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'postings', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'postings'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_postings_update AFTER UPDATE ON postings BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'postings', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'postings'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_postings_delete AFTER DELETE ON postings BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'postings', OLD.id, 'delete', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'postings'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;

    CREATE TRIGGER sync_capture_transaction_meta_insert AFTER INSERT ON transaction_meta BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'transaction_meta', NEW.transaction_id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'transaction_meta'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_transaction_meta_update AFTER UPDATE ON transaction_meta BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'transaction_meta', NEW.transaction_id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'transaction_meta'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_transaction_meta_delete AFTER DELETE ON transaction_meta BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'transaction_meta', OLD.transaction_id, 'delete', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'transaction_meta'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;

    CREATE TRIGGER sync_capture_splits_insert AFTER INSERT ON splits BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'splits', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'splits'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_splits_update AFTER UPDATE ON splits BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'splits', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'splits'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_splits_delete AFTER DELETE ON splits BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'splits', OLD.id, 'delete', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'splits'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;

    CREATE TRIGGER sync_capture_categories_insert AFTER INSERT ON categories BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'categories', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'categories'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_categories_update AFTER UPDATE ON categories BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'categories', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'categories'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_categories_delete AFTER DELETE ON categories BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'categories', OLD.id, 'delete', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'categories'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;

    CREATE TRIGGER sync_capture_category_groups_insert AFTER INSERT ON category_groups BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'category_groups', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'category_groups'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_category_groups_update AFTER UPDATE ON category_groups BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'category_groups', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'category_groups'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_category_groups_delete AFTER DELETE ON category_groups BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'category_groups', OLD.id, 'delete', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'category_groups'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;

    CREATE TRIGGER sync_capture_income_categories_insert AFTER INSERT ON income_categories BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'income_categories', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'income_categories'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_income_categories_update AFTER UPDATE ON income_categories BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'income_categories', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'income_categories'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_income_categories_delete AFTER DELETE ON income_categories BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'income_categories', OLD.id, 'delete', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'income_categories'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;

    CREATE TRIGGER sync_capture_allocation_events_insert AFTER INSERT ON allocation_events BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'allocation_events', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'allocation_events'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_allocation_events_update AFTER UPDATE ON allocation_events BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'allocation_events', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'allocation_events'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_allocation_events_delete AFTER DELETE ON allocation_events BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'allocation_events', OLD.id, 'delete', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'allocation_events'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;

    CREATE TRIGGER sync_capture_month_closes_insert AFTER INSERT ON month_closes BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'month_closes', NEW.month, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'month_closes'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_month_closes_update AFTER UPDATE ON month_closes BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'month_closes', NEW.month, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'month_closes'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_month_closes_delete AFTER DELETE ON month_closes BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'month_closes', OLD.month, 'delete', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'month_closes'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;

    CREATE TRIGGER sync_capture_goals_insert AFTER INSERT ON goals BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'goals', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'goals'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_goals_update AFTER UPDATE ON goals BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'goals', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'goals'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_goals_delete AFTER DELETE ON goals BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'goals', OLD.id, 'delete', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'goals'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;

    CREATE TRIGGER sync_capture_valuation_snapshots_insert AFTER INSERT ON valuation_snapshots BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'valuation_snapshots', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'valuation_snapshots'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_valuation_snapshots_update AFTER UPDATE ON valuation_snapshots BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'valuation_snapshots', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'valuation_snapshots'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_valuation_snapshots_delete AFTER DELETE ON valuation_snapshots BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'valuation_snapshots', OLD.id, 'delete', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'valuation_snapshots'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;

    CREATE TRIGGER sync_capture_categorization_rules_insert AFTER INSERT ON categorization_rules BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'categorization_rules', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'categorization_rules'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_categorization_rules_update AFTER UPDATE ON categorization_rules BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'categorization_rules', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'categorization_rules'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_categorization_rules_delete AFTER DELETE ON categorization_rules BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'categorization_rules', OLD.id, 'delete', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'categorization_rules'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;

    CREATE TRIGGER sync_capture_plaid_connections_insert AFTER INSERT ON plaid_connections BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'plaid_connections', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'plaid_connections'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_plaid_connections_update AFTER UPDATE ON plaid_connections BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'plaid_connections', NEW.id, 'upsert', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'plaid_connections'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
    CREATE TRIGGER sync_capture_plaid_connections_delete AFTER DELETE ON plaid_connections BEGIN
      INSERT INTO sync_pending_changes (table_name, row_id, operation, created_at)
      SELECT 'plaid_connections', OLD.id, 'delete', datetime('now')
      WHERE (SELECT flag FROM _sync_skip LIMIT 1) = 0;
      UPDATE sync_vector_clock SET local_seq = local_seq + 1
      WHERE table_name = 'plaid_connections'
        AND (SELECT flag FROM _sync_skip LIMIT 1) = 0;
    END;
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
        let mut conn = open_connection(":memory:", None).unwrap();
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
        let mut conn = open_connection(":memory:", None).unwrap();
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
        let mut conn = open_connection(":memory:", None).unwrap();
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
        let mut conn = open_connection(":memory:", None).unwrap();
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
        let mut conn = open_connection(":memory:", None).unwrap();
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
        let mut conn = open_connection(":memory:", None).unwrap();
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
        let mut conn = open_connection(":memory:", None).unwrap();
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
        let mut conn = open_connection(":memory:", None).unwrap();
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
        let mut conn = open_connection(":memory:", None).unwrap();
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
        let mut conn = open_connection(":memory:", None).unwrap();
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
        let mut conn = open_connection(":memory:", None).unwrap();
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
    fn migration_015_creates_plaid_connections() {
        let mut conn = open_connection(":memory:", None).unwrap();
        run_migrations(&mut conn).unwrap();

        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='plaid_connections'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
            > 0;
        assert!(exists, "plaid_connections table should exist after migration 15");

        for col in &[
            "id",
            "plaid_item_id",
            "access_token",
            "institution_name",
            "institution_id",
            "account_ids",
            "connected_at",
            "last_synced_at",
        ] {
            let col_exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info('plaid_connections') WHERE name=?1",
                    [col],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap()
                > 0;
            assert!(col_exists, "plaid_connections.{col} should exist after migration 15");
        }
    }

    #[test]
    fn migration_016_adds_plaid_account_id_to_accounts() {
        let mut conn = open_connection(":memory:", None).unwrap();
        run_migrations(&mut conn).unwrap();

        let col_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('accounts') WHERE name='plaid_account_id'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
            > 0;
        assert!(col_exists, "accounts.plaid_account_id should exist after migration 16");

        // Can insert an account without plaid_account_id.
        conn.execute(
            "INSERT INTO accounts (id, name, type, subtype, institution, currency, created_at)
             VALUES ('acc-1', 'Checking', 'depository', 'checking', 'Test', 'USD', datetime('now'))",
            [],
        )
        .unwrap();

        // Can insert an account with plaid_account_id.
        conn.execute(
            "INSERT INTO accounts (id, name, type, subtype, institution, currency, created_at, plaid_account_id)
             VALUES ('acc-2', 'Savings', 'depository', 'savings', 'Test', 'USD', datetime('now'), 'plaid-acct-abc')",
            [],
        )
        .unwrap();

        // Duplicate plaid_account_id must fail (partial unique index).
        let dup = conn.execute(
            "INSERT INTO accounts (id, name, type, subtype, institution, currency, created_at, plaid_account_id)
             VALUES ('acc-3', 'Savings2', 'depository', 'savings', 'Test', 'USD', datetime('now'), 'plaid-acct-abc')",
            [],
        );
        assert!(dup.is_err(), "duplicate plaid_account_id must violate unique index");

        // Two accounts with NULL plaid_account_id are allowed (partial index excludes NULLs).
        conn.execute(
            "INSERT INTO accounts (id, name, type, subtype, institution, currency, created_at)
             VALUES ('acc-4', 'Other', 'credit', 'credit card', 'Test', 'USD', datetime('now'))",
            [],
        )
        .unwrap();
    }

    #[test]
    fn migration_017_creates_sync_tables_and_clock() {
        let mut conn = open_connection(":memory:", None).unwrap();
        run_migrations(&mut conn).unwrap();

        for table in &["sync_state", "sync_pending_changes", "sync_vector_clock"] {
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap()
                > 0;
            assert!(exists, "table {table} should exist after migration 17");
        }

        let clock_rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM sync_vector_clock", [], |r| r.get(0))
            .unwrap();
        assert_eq!(clock_rows, 16, "sync_vector_clock should have 16 rows");
    }

    #[test]
    fn migration_017_insert_trigger_fires() {
        let mut conn = open_connection(":memory:", None).unwrap();
        run_migrations(&mut conn).unwrap();

        conn.execute(
            "INSERT INTO accounts (id, name, type, subtype, institution, currency, created_at)
             VALUES ('acc-1', 'Checking', 'depository', 'checking', 'Test', 'USD', datetime('now'))",
            [],
        )
        .unwrap();

        let pending: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sync_pending_changes WHERE table_name='accounts' AND operation='upsert'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(pending, 1);

        let clock: i64 = conn
            .query_row(
                "SELECT local_seq FROM sync_vector_clock WHERE table_name='accounts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(clock, 1);
    }

    #[test]
    fn migration_017_update_and_insert_produce_two_rows_clock_two() {
        let mut conn = open_connection(":memory:", None).unwrap();
        run_migrations(&mut conn).unwrap();

        conn.execute(
            "INSERT INTO accounts (id, name, type, subtype, institution, currency, created_at)
             VALUES ('acc-1', 'Checking', 'depository', 'checking', 'Test', 'USD', datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE accounts SET name = 'Updated' WHERE id = 'acc-1'",
            [],
        )
        .unwrap();

        let pending: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sync_pending_changes WHERE table_name='accounts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(pending, 2);

        let clock: i64 = conn
            .query_row(
                "SELECT local_seq FROM sync_vector_clock WHERE table_name='accounts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(clock, 2);
    }

    #[test]
    fn migration_017_delete_trigger_fires() {
        let mut conn = open_connection(":memory:", None).unwrap();
        run_migrations(&mut conn).unwrap();

        conn.execute(
            "INSERT INTO accounts (id, name, type, subtype, institution, currency, created_at)
             VALUES ('acc-1', 'Checking', 'depository', 'checking', 'Test', 'USD', datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM accounts WHERE id = 'acc-1'", []).unwrap();

        let pending: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sync_pending_changes WHERE operation='delete' AND table_name='accounts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(pending, 1);
    }

    #[test]
    fn migration_017_skip_flag_suppresses_trigger() {
        let mut conn = open_connection(":memory:", None).unwrap();
        run_migrations(&mut conn).unwrap();

        conn.execute("UPDATE _sync_skip SET flag = 1", []).unwrap();
        conn.execute(
            "INSERT INTO accounts (id, name, type, subtype, institution, currency, created_at)
             VALUES ('acc-1', 'Checking', 'depository', 'checking', 'Test', 'USD', datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute("UPDATE _sync_skip SET flag = 0", []).unwrap();

        let pending: i64 = conn
            .query_row("SELECT COUNT(*) FROM sync_pending_changes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(pending, 0);

        let clock: i64 = conn
            .query_row(
                "SELECT local_seq FROM sync_vector_clock WHERE table_name='accounts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(clock, 0);
    }

    #[test]
    fn migration_017_transaction_meta_uses_transaction_id_as_row_id() {
        let mut conn = open_connection(":memory:", None).unwrap();
        run_migrations(&mut conn).unwrap();

        conn.execute(
            "INSERT INTO accounts (id, name, type, subtype, institution, currency, created_at)
             VALUES ('acc-1', 'Checking', 'depository', 'checking', 'Test', 'USD', datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transactions (id, account_id, created_at)
             VALUES ('txn-1', 'acc-1', datetime('now'))",
            [],
        )
        .unwrap();

        let pending_before: i64 = conn
            .query_row("SELECT COUNT(*) FROM sync_pending_changes", [], |r| r.get(0))
            .unwrap();

        conn.execute(
            "INSERT INTO transaction_meta (transaction_id, notes, tags, reviewed, updated_at)
             VALUES ('txn-1', 'test note', '[]', 0, datetime('now'))",
            [],
        )
        .unwrap();

        let row_id: String = conn
            .query_row(
                "SELECT row_id FROM sync_pending_changes WHERE table_name='transaction_meta' ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(row_id, "txn-1");

        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM sync_pending_changes", [], |r| r.get(0))
            .unwrap();
        assert!(total > pending_before);
    }

    #[test]
    fn migrations_are_idempotent() {
        let mut conn = open_connection(":memory:", None).unwrap();
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
