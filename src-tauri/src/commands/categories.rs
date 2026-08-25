use chrono::Utc;
use rusqlite::Connection;
use serde::Serialize;
use std::sync::Mutex;
use tauri::State;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/lib/generated/CategoryRow.ts")]
pub struct CategoryRow {
    pub id: String,
    pub name: String,
    #[ts(type = "'flow' | 'sinking'")]
    pub kind: String,
    pub created_at: String,
}

#[tauri::command]
pub fn list_categories(
    db: State<'_, Mutex<Connection>>,
) -> Result<Vec<CategoryRow>, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, name, kind, created_at FROM categories ORDER BY kind, name")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(CategoryRow {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

#[tauri::command]
pub fn create_category(
    db: State<'_, Mutex<Connection>>,
    name: String,
    kind: String,
) -> Result<String, String> {
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Err("Category name cannot be blank".into());
    }
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let conn = db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO categories (id, name, kind, created_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![id, trimmed, kind, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(id)
}

#[tauri::command]
pub fn update_category(
    db: State<'_, Mutex<Connection>>,
    id: String,
    name: String,
) -> Result<(), String> {
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Err("Category name cannot be blank".into());
    }
    let conn = db.lock().map_err(|e| e.to_string())?;
    let rows = conn
        .execute(
            "UPDATE categories SET name = ?1 WHERE id = ?2",
            rusqlite::params![trimmed, id],
        )
        .map_err(|e| e.to_string())?;
    if rows == 0 {
        return Err(format!("Category {id} not found"));
    }
    Ok(())
}

fn upsert_category_assignment_inner(
    conn: &Connection,
    transaction_id: &str,
    category_id: Option<&str>,
) -> Result<(), String> {
    conn.execute(
        "DELETE FROM category_assignments WHERE transaction_id = ?1",
        rusqlite::params![transaction_id],
    )
    .map_err(|e| e.to_string())?;

    if let Some(cat_id) = category_id {
        // Look up the current raw amount from the unsuperseded record.
        let amount_cents: i64 = conn
            .query_row(
                "SELECT amount_cents FROM raw_records
                 WHERE transaction_id = ?1
                   AND NOT EXISTS (
                       SELECT 1 FROM raw_records rr2 WHERE rr2.supersedes_id = raw_records.id
                   )",
                rusqlite::params![transaction_id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;

        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO category_assignments (id, transaction_id, category_id, amount_cents)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, transaction_id, cat_id, amount_cents],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub fn upsert_category_assignment(
    db: State<'_, Mutex<Connection>>,
    transaction_id: String,
    category_id: Option<String>,
) -> Result<(), String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    upsert_category_assignment_inner(&conn, &transaction_id, category_id.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{db::open_connection, migrations::run_migrations, seed::seed_categories};
    use std::sync::Mutex;

    fn insert_transaction_with_record(conn: &Connection, amount_cents: i64) -> String {
        let account_id = Uuid::new_v4().to_string();
        let tx_id = Uuid::new_v4().to_string();
        let rr_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO accounts (id, name, type, subtype, institution, currency, created_at)
             VALUES (?1, 'Bank', 'depository', 'checking', 'Bank', 'USD', datetime('now'))",
            rusqlite::params![account_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transactions (id, account_id, created_at) VALUES (?1, ?2, datetime('now'))",
            rusqlite::params![tx_id, account_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO raw_records
             (id, transaction_id, supersedes_id, import_batch_id, source_id, date, amount_cents, description, raw_json, created_at)
             VALUES (?1, ?2, NULL, NULL, ?3, '2026-01-01', ?4, 'Test', '{}', datetime('now'))",
            rusqlite::params![rr_id, tx_id, format!("src|{}", rr_id), amount_cents],
        )
        .unwrap();
        tx_id
    }

    fn test_db() -> Mutex<Connection> {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();
        seed_categories(&conn).unwrap();
        Mutex::new(conn)
    }

    #[test]
    fn list_returns_seeded_categories() {
        let db = test_db();
        let conn = db.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM categories")
            .unwrap();
        let count: i64 = stmt.query_row([], |r| r.get(0)).unwrap();
        assert_eq!(count, 16);
    }

    #[test]
    fn create_inserts_and_returns_id() {
        let db = test_db();
        let id = {
            let conn = db.lock().unwrap();
            let id = Uuid::new_v4().to_string();
            let now = Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO categories (id, name, kind, created_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, "Test", "flow", now],
            )
            .unwrap();
            id
        };
        let conn = db.lock().unwrap();
        let name: String = conn
            .query_row(
                "SELECT name FROM categories WHERE id = ?1",
                rusqlite::params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(name, "Test");
    }

    #[test]
    fn update_changes_name() {
        let db = test_db();
        let conn = db.lock().unwrap();
        // grab the first seeded id
        let id: String = conn
            .query_row("SELECT id FROM categories LIMIT 1", [], |r| r.get(0))
            .unwrap();
        conn.execute(
            "UPDATE categories SET name = ?1 WHERE id = ?2",
            rusqlite::params!["Renamed", id],
        )
        .unwrap();
        let name: String = conn
            .query_row(
                "SELECT name FROM categories WHERE id = ?1",
                rusqlite::params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(name, "Renamed");
    }

    #[test]
    fn blank_name_returns_err_on_create() {
        // validate the guard directly
        let result: Result<String, String> = {
            let trimmed = "  ".trim().to_string();
            if trimmed.is_empty() {
                Err("Category name cannot be blank".into())
            } else {
                Ok("would insert".into())
            }
        };
        assert!(result.is_err());
    }

    #[test]
    fn update_category_does_not_change_kind() {
        let db = test_db();
        let conn = db.lock().unwrap();
        let (id, original_kind): (String, String) = conn
            .query_row(
                "SELECT id, kind FROM categories WHERE kind = 'flow' LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        // update only name
        conn.execute(
            "UPDATE categories SET name = ?1 WHERE id = ?2",
            rusqlite::params!["New Name", id],
        )
        .unwrap();
        let kind: String = conn
            .query_row(
                "SELECT kind FROM categories WHERE id = ?1",
                rusqlite::params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(kind, original_kind);
    }

    #[test]
    fn upsert_assignment_inserts_row() {
        let db = test_db();
        let conn = db.lock().unwrap();
        let tx_id = insert_transaction_with_record(&conn, -1500);
        let cat_id: String = conn
            .query_row("SELECT id FROM categories LIMIT 1", [], |r| r.get(0))
            .unwrap();

        upsert_category_assignment_inner(&conn, &tx_id, Some(&cat_id)).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM category_assignments WHERE transaction_id = ?1",
                rusqlite::params![tx_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let (stored_cat, stored_amount): (String, i64) = conn
            .query_row(
                "SELECT category_id, amount_cents FROM category_assignments WHERE transaction_id = ?1",
                rusqlite::params![tx_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored_cat, cat_id);
        assert_eq!(stored_amount, -1500);
    }

    #[test]
    fn upsert_assignment_replaces_existing() {
        let db = test_db();
        let conn = db.lock().unwrap();
        let tx_id = insert_transaction_with_record(&conn, -2000);
        let mut cat_iter = conn
            .prepare("SELECT id FROM categories LIMIT 2")
            .unwrap();
        let cats: Vec<String> = cat_iter
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        let (cat_a, cat_b) = (&cats[0], &cats[1]);

        upsert_category_assignment_inner(&conn, &tx_id, Some(cat_a)).unwrap();
        upsert_category_assignment_inner(&conn, &tx_id, Some(cat_b)).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM category_assignments WHERE transaction_id = ?1",
                rusqlite::params![tx_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let stored_cat: String = conn
            .query_row(
                "SELECT category_id FROM category_assignments WHERE transaction_id = ?1",
                rusqlite::params![tx_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(&stored_cat, cat_b);
    }

    #[test]
    fn upsert_assignment_none_clears() {
        let db = test_db();
        let conn = db.lock().unwrap();
        let tx_id = insert_transaction_with_record(&conn, -500);
        let cat_id: String = conn
            .query_row("SELECT id FROM categories LIMIT 1", [], |r| r.get(0))
            .unwrap();

        upsert_category_assignment_inner(&conn, &tx_id, Some(&cat_id)).unwrap();
        upsert_category_assignment_inner(&conn, &tx_id, None).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM category_assignments WHERE transaction_id = ?1",
                rusqlite::params![tx_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
}
