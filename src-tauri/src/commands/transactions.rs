use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::State;
use ts_rs::TS;

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../src/lib/generated/TransactionRow.ts")]
pub struct TransactionRow {
    pub id: String,
    pub account_id: String,
    pub account_name: String,
    pub date: String,
    // Tauri IPC JSON encodes i64 as a JS number; override bigint.
    #[ts(type = "number")]
    pub amount_cents: i64,
    pub description: String,
    pub notes: String,
    pub tags: Vec<String>,
    pub reviewed: bool,
    pub category_type: Option<String>,
    pub category_id: Option<String>,
    pub category_name: Option<String>,
}

fn list_transactions_inner(conn: &Connection) -> Result<Vec<TransactionRow>, String> {
    // The current raw_record is the one no other row supersedes.
    let mut stmt = conn
        .prepare(
            "SELECT t.id, t.account_id, a.name,
                    rr.date, rr.amount_cents, rr.description,
                    COALESCE(tm.notes, '') AS notes,
                    COALESCE(tm.tags, '[]') AS tags,
                    COALESCE(tm.reviewed, 0) AS reviewed,
                    s.target_type AS category_type,
                    s.target_id AS category_id,
                    COALESCE(cat.name, ic.name) AS category_name
             FROM transactions t
             JOIN accounts a ON t.account_id = a.id
             JOIN raw_records rr ON rr.transaction_id = t.id
               AND NOT EXISTS (
                   SELECT 1 FROM raw_records rr2 WHERE rr2.supersedes_id = rr.id
               )
             LEFT JOIN transaction_meta tm ON tm.transaction_id = t.id
             LEFT JOIN splits s ON s.transaction_id = t.id
             LEFT JOIN categories cat ON cat.id = s.target_id AND s.target_type = 'envelope'
             LEFT JOIN income_categories ic ON ic.id = s.target_id AND s.target_type = 'income'
             ORDER BY rr.date DESC, t.created_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            let tags_json: String = row.get(7)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                tags_json,
                row.get::<_, i64>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, Option<String>>(11)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for row in rows {
        let (
            id,
            account_id,
            account_name,
            date,
            amount_cents,
            description,
            notes,
            tags_json,
            reviewed_int,
            category_type,
            category_id,
            category_name,
        ) = row.map_err(|e| e.to_string())?;
        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
        result.push(TransactionRow {
            id,
            account_id,
            account_name,
            date,
            amount_cents,
            description,
            notes,
            tags,
            reviewed: reviewed_int != 0,
            category_type,
            category_id,
            category_name,
        });
    }

    Ok(result)
}

#[derive(Debug, Deserialize)]
pub struct UpsertMetaArgs {
    pub transaction_id: String,
    pub notes: String,
    pub tags: Vec<String>,
    pub reviewed: bool,
}

fn upsert_transaction_meta_inner(conn: &Connection, args: &UpsertMetaArgs) -> Result<(), String> {
    let tags_json = serde_json::to_string(&args.tags).map_err(|e| e.to_string())?;
    let reviewed_int: i64 = if args.reviewed { 1 } else { 0 };

    conn.execute(
        "INSERT INTO transaction_meta (transaction_id, notes, tags, reviewed, updated_at)
         VALUES (?1, ?2, ?3, ?4, datetime('now'))
         ON CONFLICT(transaction_id) DO UPDATE SET
             notes = excluded.notes,
             tags = excluded.tags,
             reviewed = excluded.reviewed,
             updated_at = excluded.updated_at",
        rusqlite::params![args.transaction_id, args.notes, tags_json, reviewed_int],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn list_transactions(db: State<Mutex<Connection>>) -> Result<Vec<TransactionRow>, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    list_transactions_inner(&conn)
}

#[tauri::command]
pub fn upsert_transaction_meta(
    db: State<Mutex<Connection>>,
    transaction_id: String,
    notes: String,
    tags: Vec<String>,
    reviewed: bool,
) -> Result<(), String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    upsert_transaction_meta_inner(
        &conn,
        &UpsertMetaArgs {
            transaction_id,
            notes,
            tags,
            reviewed,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::open_connection;
    use crate::storage::migrations::run_migrations;
    use uuid::Uuid;

    fn setup() -> Connection {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();
        conn
    }

    fn insert_account(conn: &Connection) -> String {
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO accounts (id, name, type, subtype, institution, currency, created_at)
             VALUES (?1, 'Test Bank', 'depository', 'checking', 'Bank', 'USD', datetime('now'))",
            rusqlite::params![id],
        )
        .unwrap();
        id
    }

    fn insert_transaction_with_record(
        conn: &Connection,
        account_id: &str,
        date: &str,
        amount_cents: i64,
        description: &str,
    ) -> String {
        let tx_id = Uuid::new_v4().to_string();
        let rr_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO transactions (id, account_id, created_at) VALUES (?1, ?2, datetime('now'))",
            rusqlite::params![tx_id, account_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO raw_records
             (id, transaction_id, supersedes_id, import_batch_id, source_id, date, amount_cents, description, raw_json, created_at)
             VALUES (?1, ?2, NULL, NULL, ?3, ?4, ?5, ?6, '{}', datetime('now'))",
            rusqlite::params![rr_id, tx_id, format!("test|{}", rr_id), date, amount_cents, description],
        )
        .unwrap();
        tx_id
    }

    #[test]
    fn list_returns_row_with_default_empty_meta() {
        let conn = setup();
        let account_id = insert_account(&conn);
        insert_transaction_with_record(&conn, &account_id, "2026-01-15", -4567, "Grocery Store");

        let rows = list_transactions_inner(&conn).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].notes, "");
        assert!(rows[0].tags.is_empty());
        assert!(!rows[0].reviewed);
        assert!(rows[0].category_id.is_none());
        assert!(rows[0].category_name.is_none());
    }

    #[test]
    fn list_returns_category_when_assigned() {
        let conn = setup();
        let account_id = insert_account(&conn);
        let tx_id =
            insert_transaction_with_record(&conn, &account_id, "2026-01-15", -1000, "Trader Joe's");

        let cat_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO categories (id, name, kind, created_at) VALUES (?1, 'Groceries', 'flow', datetime('now'))",
            rusqlite::params![cat_id],
        )
        .unwrap();
        let split_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO splits (id, transaction_id, target_type, target_id, amount_cents) VALUES (?1, ?2, 'envelope', ?3, ?4)",
            rusqlite::params![split_id, tx_id, cat_id, -1000_i64],
        )
        .unwrap();

        let rows = list_transactions_inner(&conn).unwrap();
        let row = rows.iter().find(|r| r.id == tx_id).unwrap();
        assert_eq!(row.category_type.as_deref(), Some("envelope"));
        assert_eq!(row.category_id.as_deref(), Some(cat_id.as_str()));
        assert_eq!(row.category_name.as_deref(), Some("Groceries"));
    }

    #[test]
    fn list_returns_category_type_when_income_split_assigned() {
        let conn = setup();
        let account_id = insert_account(&conn);
        let tx_id =
            insert_transaction_with_record(&conn, &account_id, "2026-01-15", 480_000, "Paycheck");

        let ic_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO income_categories (id, name, hidden, created_at) VALUES (?1, 'Paycheck', 0, datetime('now'))",
            rusqlite::params![ic_id],
        )
        .unwrap();
        let split_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO splits (id, transaction_id, target_type, target_id, amount_cents) VALUES (?1, ?2, 'income', ?3, ?4)",
            rusqlite::params![split_id, tx_id, ic_id, 480_000_i64],
        )
        .unwrap();

        let rows = list_transactions_inner(&conn).unwrap();
        let row = rows.iter().find(|r| r.id == tx_id).unwrap();
        assert_eq!(row.category_type.as_deref(), Some("income"));
        assert_eq!(row.category_id.as_deref(), Some(ic_id.as_str()));
        assert_eq!(row.category_name.as_deref(), Some("Paycheck"));
    }

    #[test]
    fn list_returns_correct_joined_fields() {
        let conn = setup();
        let account_id = insert_account(&conn);
        insert_transaction_with_record(&conn, &account_id, "2026-03-10", -2000, "Coffee Shop");

        let rows = list_transactions_inner(&conn).unwrap();
        assert_eq!(rows[0].date, "2026-03-10");
        assert_eq!(rows[0].amount_cents, -2000);
        assert_eq!(rows[0].description, "Coffee Shop");
        assert_eq!(rows[0].account_name, "Test Bank");
        assert_eq!(rows[0].account_id, account_id);
    }

    #[test]
    fn upsert_creates_meta_row() {
        let conn = setup();
        let account_id = insert_account(&conn);
        let tx_id = insert_transaction_with_record(&conn, &account_id, "2026-01-01", -500, "Lunch");

        upsert_transaction_meta_inner(
            &conn,
            &UpsertMetaArgs {
                transaction_id: tx_id.clone(),
                notes: "work meal".to_string(),
                tags: vec!["food".to_string(), "work".to_string()],
                reviewed: true,
            },
        )
        .unwrap();

        let rows = list_transactions_inner(&conn).unwrap();
        let row = rows.iter().find(|r| r.id == tx_id).unwrap();
        assert_eq!(row.notes, "work meal");
        assert_eq!(row.tags, vec!["food", "work"]);
        assert!(row.reviewed);
    }

    #[test]
    fn upsert_updates_existing_meta_row() {
        let conn = setup();
        let account_id = insert_account(&conn);
        let tx_id = insert_transaction_with_record(&conn, &account_id, "2026-01-01", -500, "Lunch");

        upsert_transaction_meta_inner(
            &conn,
            &UpsertMetaArgs {
                transaction_id: tx_id.clone(),
                notes: "first note".to_string(),
                tags: vec!["old".to_string()],
                reviewed: false,
            },
        )
        .unwrap();

        upsert_transaction_meta_inner(
            &conn,
            &UpsertMetaArgs {
                transaction_id: tx_id.clone(),
                notes: "updated note".to_string(),
                tags: vec!["new".to_string()],
                reviewed: true,
            },
        )
        .unwrap();

        let rows = list_transactions_inner(&conn).unwrap();
        let row = rows.iter().find(|r| r.id == tx_id).unwrap();
        assert_eq!(row.notes, "updated note");
        assert_eq!(row.tags, vec!["new"]);
        assert!(row.reviewed);

        let meta_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM transaction_meta WHERE transaction_id = ?1",
                rusqlite::params![tx_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(meta_count, 1);
    }

    #[test]
    fn list_returns_updated_meta() {
        let conn = setup();
        let account_id = insert_account(&conn);
        let tx_id =
            insert_transaction_with_record(&conn, &account_id, "2026-02-14", -3000, "Dinner");

        let before = list_transactions_inner(&conn).unwrap();
        assert_eq!(before[0].notes, "");

        upsert_transaction_meta_inner(
            &conn,
            &UpsertMetaArgs {
                transaction_id: tx_id,
                notes: "valentines".to_string(),
                tags: vec![],
                reviewed: false,
            },
        )
        .unwrap();

        let after = list_transactions_inner(&conn).unwrap();
        assert_eq!(after[0].notes, "valentines");
    }
}
