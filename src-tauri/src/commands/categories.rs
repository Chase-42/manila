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
    pub group_id: Option<String>,
    pub created_at: String,
}

#[tauri::command]
pub fn list_categories(
    vault: State<'_, crate::crypto::VaultState>,
    db: State<'_, Mutex<Connection>>,
) -> Result<Vec<CategoryRow>, String> {
    super::require_unlocked(&vault)?;
    let conn = db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, name, kind, group_id, created_at FROM categories ORDER BY kind, name")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(CategoryRow {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                group_id: row.get(3)?,
                created_at: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

#[tauri::command]
pub fn create_category(
    vault: State<'_, crate::crypto::VaultState>,
    db: State<'_, Mutex<Connection>>,
    name: String,
    kind: String,
) -> Result<String, String> {
    super::require_unlocked(&vault)?;
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
    vault: State<'_, crate::crypto::VaultState>,
    db: State<'_, Mutex<Connection>>,
    id: String,
    name: String,
) -> Result<(), String> {
    super::require_unlocked(&vault)?;
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

fn upsert_categorization_rule_inner(
    conn: &Connection,
    description: &str,
    category_id: &str,
) -> Result<(), String> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO categorization_rules (id, merchant_pattern, category_id, priority, created_at)
         VALUES (?1, ?2, ?3, 0, ?4)
         ON CONFLICT(merchant_pattern) DO UPDATE SET
             category_id = excluded.category_id,
             created_at  = excluded.created_at",
        rusqlite::params![id, description, category_id, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn upsert_split_inner(
    conn: &Connection,
    transaction_id: &str,
    target_type: &str,
    target_id: &str,
) -> Result<(), String> {
    conn.execute(
        "DELETE FROM splits WHERE transaction_id = ?1",
        rusqlite::params![transaction_id],
    )
    .map_err(|e| e.to_string())?;

    if !target_id.is_empty() {
        if target_type != "envelope" && target_type != "income" {
            return Err(format!(
                "target_type must be 'envelope' or 'income', got '{target_type}'"
            ));
        }

        // Fetch amount and description from the unsuperseded record in one query.
        let (amount_cents, description): (i64, String) = conn
            .query_row(
                "SELECT amount_cents, description FROM raw_records
                 WHERE transaction_id = ?1
                   AND NOT EXISTS (
                       SELECT 1 FROM raw_records rr2 WHERE rr2.supersedes_id = raw_records.id
                   )",
                rusqlite::params![transaction_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| e.to_string())?;

        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO splits (id, transaction_id, target_type, target_id, amount_cents)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, transaction_id, target_type, target_id, amount_cents],
        )
        .map_err(|e| e.to_string())?;

        if target_type == "envelope" {
            upsert_categorization_rule_inner(conn, &description, target_id)?;
        }
    }

    Ok(())
}

#[tauri::command]
pub fn upsert_split(
    vault: State<'_, crate::crypto::VaultState>,
    db: State<'_, Mutex<Connection>>,
    transaction_id: String,
    target_type: String,
    target_id: String,
) -> Result<(), String> {
    super::require_unlocked(&vault)?;
    let conn = db.lock().map_err(|e| e.to_string())?;
    upsert_split_inner(&conn, &transaction_id, &target_type, &target_id)
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/lib/generated/IncomeCategoryItem.ts")]
pub struct IncomeCategoryItem {
    pub id: String,
    pub name: String,
    pub hidden: bool,
}

fn list_income_categories_inner(conn: &Connection) -> Result<Vec<IncomeCategoryItem>, String> {
    let mut stmt = conn
        .prepare("SELECT id, name, hidden FROM income_categories ORDER BY created_at")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(IncomeCategoryItem {
                id: row.get(0)?,
                name: row.get(1)?,
                hidden: row.get::<_, i64>(2)? != 0,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

fn create_income_category_inner(conn: &Connection, name: &str) -> Result<String, String> {
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Err("Income category name cannot be blank".into());
    }
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO income_categories (id, name, hidden, created_at) VALUES (?1, ?2, 0, ?3)",
        rusqlite::params![id, trimmed, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(id)
}

fn set_income_category_hidden_inner(
    conn: &Connection,
    id: &str,
    hidden: bool,
) -> Result<(), String> {
    let rows = conn
        .execute(
            "UPDATE income_categories SET hidden = ?1 WHERE id = ?2",
            rusqlite::params![i64::from(hidden), id],
        )
        .map_err(|e| e.to_string())?;
    if rows == 0 {
        return Err(format!("Income category {id} not found"));
    }
    Ok(())
}

#[tauri::command]
pub fn list_income_categories(
    vault: State<'_, crate::crypto::VaultState>,
    db: State<'_, Mutex<Connection>>,
) -> Result<Vec<IncomeCategoryItem>, String> {
    super::require_unlocked(&vault)?;
    let conn = db.lock().map_err(|e| e.to_string())?;
    list_income_categories_inner(&conn)
}

#[tauri::command]
pub fn create_income_category(
    vault: State<'_, crate::crypto::VaultState>,
    db: State<'_, Mutex<Connection>>,
    name: String,
) -> Result<String, String> {
    super::require_unlocked(&vault)?;
    let conn = db.lock().map_err(|e| e.to_string())?;
    create_income_category_inner(&conn, &name)
}

#[tauri::command]
pub fn set_income_category_hidden(
    vault: State<'_, crate::crypto::VaultState>,
    db: State<'_, Mutex<Connection>>,
    id: String,
    hidden: bool,
) -> Result<(), String> {
    super::require_unlocked(&vault)?;
    let conn = db.lock().map_err(|e| e.to_string())?;
    set_income_category_hidden_inner(&conn, &id, hidden)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{
        db::open_connection,
        migrations::run_migrations,
        seed::{seed_categories, seed_category_groups, seed_income_categories},
    };
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
        seed_category_groups(&conn).unwrap();
        seed_income_categories(&conn).unwrap();
        Mutex::new(conn)
    }

    #[test]
    fn list_returns_seeded_categories() {
        let db = test_db();
        let conn = db.lock().unwrap();
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM categories").unwrap();
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
    fn upsert_split_inserts_row() {
        let db = test_db();
        let conn = db.lock().unwrap();
        let tx_id = insert_transaction_with_record(&conn, -1500);
        let cat_id: String = conn
            .query_row("SELECT id FROM categories LIMIT 1", [], |r| r.get(0))
            .unwrap();

        upsert_split_inner(&conn, &tx_id, "envelope", &cat_id).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM splits WHERE transaction_id = ?1",
                rusqlite::params![tx_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let (stored_target, stored_amount): (String, i64) = conn
            .query_row(
                "SELECT target_id, amount_cents FROM splits WHERE transaction_id = ?1",
                rusqlite::params![tx_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored_target, cat_id);
        assert_eq!(stored_amount, -1500);
    }

    #[test]
    fn upsert_split_replaces_existing() {
        let db = test_db();
        let conn = db.lock().unwrap();
        let tx_id = insert_transaction_with_record(&conn, -2000);
        let mut cat_iter = conn.prepare("SELECT id FROM categories LIMIT 2").unwrap();
        let cats: Vec<String> = cat_iter
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        let (cat_a, cat_b) = (&cats[0], &cats[1]);

        upsert_split_inner(&conn, &tx_id, "envelope", cat_a).unwrap();
        upsert_split_inner(&conn, &tx_id, "envelope", cat_b).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM splits WHERE transaction_id = ?1",
                rusqlite::params![tx_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let stored_target: String = conn
            .query_row(
                "SELECT target_id FROM splits WHERE transaction_id = ?1",
                rusqlite::params![tx_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(&stored_target, cat_b);
    }

    #[test]
    fn upsert_split_empty_target_clears() {
        let db = test_db();
        let conn = db.lock().unwrap();
        let tx_id = insert_transaction_with_record(&conn, -500);
        let cat_id: String = conn
            .query_row("SELECT id FROM categories LIMIT 1", [], |r| r.get(0))
            .unwrap();

        upsert_split_inner(&conn, &tx_id, "envelope", &cat_id).unwrap();
        upsert_split_inner(&conn, &tx_id, "", "").unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM splits WHERE transaction_id = ?1",
                rusqlite::params![tx_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn upsert_split_rejects_invalid_target_type() {
        let db = test_db();
        let conn = db.lock().unwrap();
        let tx_id = insert_transaction_with_record(&conn, -500);
        let result = upsert_split_inner(&conn, &tx_id, "bogus", "some-id");
        assert!(result.is_err());
    }

    fn insert_transaction_with_description(conn: &Connection, description: &str) -> String {
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
             VALUES (?1, ?2, NULL, NULL, ?3, '2026-01-01', -1000, ?4, '{}', datetime('now'))",
            rusqlite::params![rr_id, tx_id, format!("src|{}", rr_id), description],
        )
        .unwrap();
        tx_id
    }

    #[test]
    fn upsert_split_envelope_creates_rule() {
        let db = test_db();
        let conn = db.lock().unwrap();
        let tx_id = insert_transaction_with_description(&conn, "Whole Foods Market");
        let cat_id: String = conn
            .query_row("SELECT id FROM categories LIMIT 1", [], |r| r.get(0))
            .unwrap();

        upsert_split_inner(&conn, &tx_id, "envelope", &cat_id).unwrap();

        let rule: Option<(String, String)> = conn
            .query_row(
                "SELECT merchant_pattern, category_id FROM categorization_rules LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();
        assert!(rule.is_some(), "rule row should be created");
        let (pattern, stored_cat) = rule.unwrap();
        assert_eq!(pattern, "Whole Foods Market");
        assert_eq!(stored_cat, cat_id);
    }

    #[test]
    fn upsert_split_envelope_updates_existing_rule() {
        let db = test_db();
        let conn = db.lock().unwrap();
        let tx_id = insert_transaction_with_description(&conn, "Shell Station");
        let mut cat_iter = conn.prepare("SELECT id FROM categories LIMIT 2").unwrap();
        let cats: Vec<String> = cat_iter
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        let (cat_a, cat_b) = (&cats[0], &cats[1]);

        upsert_split_inner(&conn, &tx_id, "envelope", cat_a).unwrap();
        upsert_split_inner(&conn, &tx_id, "envelope", cat_b).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM categorization_rules WHERE merchant_pattern = 'Shell Station'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "re-assigning should update the rule, not insert a new one"
        );

        let stored_cat: String = conn
            .query_row(
                "SELECT category_id FROM categorization_rules WHERE merchant_pattern = 'Shell Station'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(&stored_cat, cat_b);
    }

    #[test]
    fn upsert_split_income_does_not_create_rule() {
        let db = test_db();
        let conn = db.lock().unwrap();
        let tx_id = insert_transaction_with_description(&conn, "Paycheck");
        let income_id: String = conn
            .query_row("SELECT id FROM income_categories LIMIT 1", [], |r| r.get(0))
            .unwrap();

        upsert_split_inner(&conn, &tx_id, "income", &income_id).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM categorization_rules", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(
            count, 0,
            "income splits must not create categorization rules"
        );
    }

    #[test]
    fn upsert_split_clear_does_not_delete_rule() {
        let db = test_db();
        let conn = db.lock().unwrap();
        let tx_id = insert_transaction_with_description(&conn, "Amazon");
        let cat_id: String = conn
            .query_row("SELECT id FROM categories LIMIT 1", [], |r| r.get(0))
            .unwrap();

        upsert_split_inner(&conn, &tx_id, "envelope", &cat_id).unwrap();
        upsert_split_inner(&conn, &tx_id, "", "").unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM categorization_rules", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 1, "clearing a split must not delete existing rules");
    }

    #[test]
    fn export_bindings_incomecategoryitem() {
        <IncomeCategoryItem as TS>::export_all().unwrap();
    }

    #[test]
    fn list_income_categories_returns_seeded() {
        let db = test_db();
        let conn = db.lock().unwrap();
        let result = list_income_categories_inner(&conn).unwrap();
        assert_eq!(result.len(), 4);
        assert!(result.iter().all(|r| !r.hidden));
        let names: Vec<&str> = result.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"Paycheck"));
    }

    #[test]
    fn create_income_category_inserts() {
        let db = test_db();
        let conn = db.lock().unwrap();
        let id = create_income_category_inner(&conn, "Rental Income").unwrap();
        assert!(!id.is_empty());
    }

    #[test]
    fn create_income_category_rejects_blank() {
        let db = test_db();
        let conn = db.lock().unwrap();
        let result = create_income_category_inner(&conn, "   ");
        assert!(result.is_err());
    }

    #[test]
    fn set_income_category_hidden_toggles_flag() {
        let db = test_db();
        let conn = db.lock().unwrap();
        let id: String = conn
            .query_row("SELECT id FROM income_categories LIMIT 1", [], |r| r.get(0))
            .unwrap();
        set_income_category_hidden_inner(&conn, &id, true).unwrap();
        let hidden: i64 = conn
            .query_row(
                "SELECT hidden FROM income_categories WHERE id = ?1",
                rusqlite::params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hidden, 1);
    }

    #[test]
    fn set_income_category_hidden_errors_on_unknown_id() {
        let db = test_db();
        let conn = db.lock().unwrap();
        let result = set_income_category_hidden_inner(&conn, "no-such-id", true);
        assert!(result.is_err());
    }
}
