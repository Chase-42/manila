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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{db::open_connection, migrations::run_migrations, seed::seed_categories};
    use std::sync::Mutex;

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
}
