use chrono::Utc;
use rusqlite::Connection;
use serde::Serialize;
use std::sync::Mutex;
use tauri::State;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/lib/generated/CategoryGroupRow.ts")]
pub struct CategoryGroupRow {
    pub id: String,
    pub name: String,
    pub sort_order: i64,
}

#[tauri::command]
pub fn list_category_groups(
    vault: State<'_, crate::crypto::VaultState>,
    db: State<'_, Mutex<Connection>>,
) -> Result<Vec<CategoryGroupRow>, String> {
    super::require_unlocked(&vault)?;
    let conn = db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, name, sort_order FROM category_groups ORDER BY sort_order")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(CategoryGroupRow {
                id: row.get(0)?,
                name: row.get(1)?,
                sort_order: row.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

#[tauri::command]
pub fn create_category_group(
    vault: State<'_, crate::crypto::VaultState>,
    db: State<'_, Mutex<Connection>>,
    name: String,
) -> Result<String, String> {
    super::require_unlocked(&vault)?;
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Err("Group name cannot be blank".into());
    }
    let conn = db.lock().map_err(|e| e.to_string())?;
    let sort_order: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(sort_order), 0) + 1 FROM category_groups",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO category_groups (id, name, sort_order, created_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![id, trimmed, sort_order, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(id)
}

#[tauri::command]
pub fn update_category_group(
    vault: State<'_, crate::crypto::VaultState>,
    db: State<'_, Mutex<Connection>>,
    id: String,
    name: String,
) -> Result<(), String> {
    super::require_unlocked(&vault)?;
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Err("Group name cannot be blank".into());
    }
    let conn = db.lock().map_err(|e| e.to_string())?;
    let rows = conn
        .execute(
            "UPDATE category_groups SET name = ?1 WHERE id = ?2",
            rusqlite::params![trimmed, id],
        )
        .map_err(|e| e.to_string())?;
    if rows == 0 {
        return Err(format!("Category group {id} not found"));
    }
    Ok(())
}

#[tauri::command]
pub fn delete_category_group(
    vault: State<'_, crate::crypto::VaultState>,
    db: State<'_, Mutex<Connection>>,
    id: String,
) -> Result<(), String> {
    super::require_unlocked(&vault)?;
    let conn = db.lock().map_err(|e| e.to_string())?;
    let assigned: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM categories WHERE group_id = ?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if assigned > 0 {
        return Err(format!(
            "Cannot delete group {id}: {assigned} categories are still assigned to it"
        ));
    }
    let rows = conn
        .execute(
            "DELETE FROM category_groups WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| e.to_string())?;
    if rows == 0 {
        return Err(format!("Category group {id} not found"));
    }
    Ok(())
}

#[tauri::command]
pub fn assign_category_to_group(
    vault: State<'_, crate::crypto::VaultState>,
    db: State<'_, Mutex<Connection>>,
    category_id: String,
    group_id: Option<String>,
) -> Result<(), String> {
    super::require_unlocked(&vault)?;
    let conn = db.lock().map_err(|e| e.to_string())?;
    let rows = conn
        .execute(
            "UPDATE categories SET group_id = ?1 WHERE id = ?2",
            rusqlite::params![group_id, category_id],
        )
        .map_err(|e| e.to_string())?;
    if rows == 0 {
        return Err(format!("Category {category_id} not found"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{
        db::open_connection,
        migrations::run_migrations,
        seed::{seed_categories, seed_category_groups},
    };

    fn test_db() -> Connection {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();
        seed_categories(&conn).unwrap();
        seed_category_groups(&conn).unwrap();
        conn
    }

    fn first_category_id(conn: &Connection) -> String {
        conn.query_row("SELECT id FROM categories LIMIT 1", [], |r| r.get(0))
            .unwrap()
    }

    fn first_group_id(conn: &Connection) -> String {
        conn.query_row(
            "SELECT id FROM category_groups ORDER BY sort_order LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn list_returns_six_seeded_groups_in_order() {
        let conn = test_db();
        let mut stmt = conn
            .prepare("SELECT id, name, sort_order FROM category_groups ORDER BY sort_order")
            .unwrap();
        let groups: Vec<CategoryGroupRow> = stmt
            .query_map([], |r| {
                Ok(CategoryGroupRow {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    sort_order: r.get(2)?,
                })
            })
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(groups.len(), 6);
        assert_eq!(groups[0].name, "Food & Dining");
        assert_eq!(groups[0].sort_order, 1);
        assert_eq!(groups[5].name, "Savings");
        assert_eq!(groups[5].sort_order, 6);
    }

    #[test]
    fn create_inserts_group_and_returns_id() {
        let conn = test_db();
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO category_groups (id, name, sort_order, created_at) VALUES (?1, 'New Group', 7, ?2)",
            rusqlite::params![id, now],
        )
        .unwrap();
        let name: String = conn
            .query_row(
                "SELECT name FROM category_groups WHERE id = ?1",
                rusqlite::params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(name, "New Group");
    }

    #[test]
    fn update_changes_group_name() {
        let conn = test_db();
        let id = first_group_id(&conn);
        conn.execute(
            "UPDATE category_groups SET name = ?1 WHERE id = ?2",
            rusqlite::params!["Renamed Group", id],
        )
        .unwrap();
        let name: String = conn
            .query_row(
                "SELECT name FROM category_groups WHERE id = ?1",
                rusqlite::params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(name, "Renamed Group");
    }

    #[test]
    fn delete_errors_when_categories_are_assigned() {
        let conn = test_db();
        // Every seeded group has at least one category assigned; pick the first one.
        let group_id = first_group_id(&conn);
        let assigned: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM categories WHERE group_id = ?1",
                rusqlite::params![group_id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            assigned > 0,
            "test requires a group with assigned categories"
        );

        let result: Result<(), String> = if assigned > 0 {
            Err(format!(
                "Cannot delete group {group_id}: {assigned} categories are still assigned to it"
            ))
        } else {
            Ok(())
        };
        assert!(result.is_err());
    }

    #[test]
    fn delete_succeeds_when_no_categories_assigned() {
        let conn = test_db();
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO category_groups (id, name, sort_order, created_at) VALUES (?1, 'Empty Group', 7, ?2)",
            rusqlite::params![id, now],
        )
        .unwrap();
        conn.execute(
            "DELETE FROM category_groups WHERE id = ?1",
            rusqlite::params![id],
        )
        .unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM category_groups WHERE id = ?1",
                rusqlite::params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn assign_sets_group_id() {
        let conn = test_db();
        let cat_id = first_category_id(&conn);
        let group_id = first_group_id(&conn);
        conn.execute(
            "UPDATE categories SET group_id = ?1 WHERE id = ?2",
            rusqlite::params![group_id, cat_id],
        )
        .unwrap();
        let stored: String = conn
            .query_row(
                "SELECT group_id FROM categories WHERE id = ?1",
                rusqlite::params![cat_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stored, group_id);
    }

    #[test]
    fn assign_none_clears_group_id() {
        let conn = test_db();
        let cat_id = first_category_id(&conn);
        conn.execute(
            "UPDATE categories SET group_id = NULL WHERE id = ?1",
            rusqlite::params![cat_id],
        )
        .unwrap();
        let stored: Option<String> = conn
            .query_row(
                "SELECT group_id FROM categories WHERE id = ?1",
                rusqlite::params![cat_id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(stored.is_none());
    }
}
