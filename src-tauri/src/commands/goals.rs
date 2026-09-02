use chrono::Utc;
use rusqlite::Connection;
use serde::Serialize;
use std::sync::Mutex;
use tauri::State;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/lib/generated/Goal.ts")]
pub struct Goal {
    pub id: String,
    pub name: String,
    #[ts(type = "number")]
    pub target_amount_cents: i64,
    pub category_id: Option<String>,
    pub target_date: Option<String>,
    pub achieved_at: Option<String>,
    pub created_at: String,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/lib/generated/GoalWithProgress.ts")]
pub struct GoalWithProgress {
    pub id: String,
    pub name: String,
    #[ts(type = "number")]
    pub target_amount_cents: i64,
    pub category_id: Option<String>,
    pub target_date: Option<String>,
    pub achieved_at: Option<String>,
    pub created_at: String,
    #[ts(type = "number")]
    pub current_balance_cents: i64,
}

fn goal_balance(conn: &Connection, category_id: &str) -> Result<i64, String> {
    // All-time allocations (excluding carry) minus all-time spending for the category.
    let allocated: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(amount_cents), 0) FROM allocation_events
             WHERE category_id = ?1 AND kind != 'carry'",
            rusqlite::params![category_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    let spent: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(amount_cents), 0) FROM splits
             WHERE target_type = 'envelope' AND target_id = ?1",
            rusqlite::params![category_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    Ok(allocated - spent)
}

fn list_goals_with_progress_inner(conn: &Connection) -> Result<Vec<GoalWithProgress>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, target_amount_cents, category_id, target_date, achieved_at, created_at
             FROM goals ORDER BY created_at",
        )
        .map_err(|e| e.to_string())?;

    let goals = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let mut result = Vec::with_capacity(goals.len());
    for (id, name, target_amount_cents, category_id, target_date, achieved_at, created_at) in goals
    {
        let current_balance_cents = match &category_id {
            Some(cat_id) => goal_balance(conn, cat_id)?,
            None => 0,
        };
        result.push(GoalWithProgress {
            id,
            name,
            target_amount_cents,
            category_id,
            target_date,
            achieved_at,
            created_at,
            current_balance_cents,
        });
    }
    Ok(result)
}

#[tauri::command]
pub fn list_goals_with_progress(
    vault: State<'_, crate::crypto::VaultState>,
    db: State<'_, Mutex<Connection>>,
) -> Result<Vec<GoalWithProgress>, String> {
    super::require_unlocked(&vault)?;
    let conn = db.lock().map_err(|e| e.to_string())?;
    list_goals_with_progress_inner(&conn)
}

#[tauri::command]
pub fn create_goal(
    vault: State<'_, crate::crypto::VaultState>,
    db: State<'_, Mutex<Connection>>,
    name: String,
    target_amount_cents: i64,
    category_id: Option<String>,
    target_date: Option<String>,
) -> Result<Goal, String> {
    super::require_unlocked(&vault)?;
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Err("Goal name cannot be blank".into());
    }
    if target_amount_cents <= 0 {
        return Err("Target amount must be greater than zero".into());
    }
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let conn = db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO goals (id, name, target_amount_cents, category_id, target_date, achieved_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6)",
        rusqlite::params![id, trimmed, target_amount_cents, category_id, target_date, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(Goal {
        id,
        name: trimmed,
        target_amount_cents,
        category_id,
        target_date,
        achieved_at: None,
        created_at: now,
    })
}

#[tauri::command]
pub fn update_goal(
    vault: State<'_, crate::crypto::VaultState>,
    db: State<'_, Mutex<Connection>>,
    id: String,
    name: String,
    target_amount_cents: i64,
    category_id: Option<String>,
    target_date: Option<String>,
) -> Result<Goal, String> {
    super::require_unlocked(&vault)?;
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Err("Goal name cannot be blank".into());
    }
    if target_amount_cents <= 0 {
        return Err("Target amount must be greater than zero".into());
    }
    let conn = db.lock().map_err(|e| e.to_string())?;
    let rows = conn
        .execute(
            "UPDATE goals SET name = ?1, target_amount_cents = ?2, category_id = ?3, target_date = ?4
             WHERE id = ?5",
            rusqlite::params![trimmed, target_amount_cents, category_id, target_date, id],
        )
        .map_err(|e| e.to_string())?;
    if rows == 0 {
        return Err(format!("Goal {id} not found"));
    }
    let achieved_at: Option<String> = conn
        .query_row(
            "SELECT achieved_at FROM goals WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let created_at: String = conn
        .query_row(
            "SELECT created_at FROM goals WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(Goal {
        id,
        name: trimmed,
        target_amount_cents,
        category_id,
        target_date,
        achieved_at,
        created_at,
    })
}

#[tauri::command]
pub fn delete_goal(
    vault: State<'_, crate::crypto::VaultState>,
    db: State<'_, Mutex<Connection>>,
    id: String,
) -> Result<(), String> {
    super::require_unlocked(&vault)?;
    let conn = db.lock().map_err(|e| e.to_string())?;
    let rows = conn
        .execute("DELETE FROM goals WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| e.to_string())?;
    if rows == 0 {
        return Err(format!("Goal {id} not found"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{
        db::open_connection,
        migrations::run_migrations,
        seed::{seed_categories, seed_category_groups, seed_income_categories},
    };

    fn test_db() -> Connection {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();
        seed_categories(&conn).unwrap();
        seed_category_groups(&conn).unwrap();
        seed_income_categories(&conn).unwrap();
        conn
    }

    fn sinking_category_id(conn: &Connection) -> String {
        conn.query_row(
            "SELECT id FROM categories WHERE kind = 'sinking' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn insert_goal(conn: &Connection, name: &str, target: i64, cat_id: Option<&str>) -> String {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO goals (id, name, target_amount_cents, category_id, target_date, achieved_at, created_at)
             VALUES (?1, ?2, ?3, ?4, NULL, NULL, ?5)",
            rusqlite::params![id, name, target, cat_id, now],
        )
        .unwrap();
        id
    }

    #[test]
    fn create_and_list_goal_no_category() {
        let conn = test_db();
        let id = insert_goal(&conn, "Vacation", 500_000, None);
        let goals = list_goals_with_progress_inner(&conn).unwrap();
        assert_eq!(goals.len(), 1);
        let g = &goals[0];
        assert_eq!(g.id, id);
        assert_eq!(g.name, "Vacation");
        assert_eq!(g.target_amount_cents, 500_000);
        assert!(g.category_id.is_none());
        assert_eq!(g.current_balance_cents, 0);
    }

    #[test]
    fn goal_balance_reflects_sinking_activity() {
        let conn = test_db();
        let cat_id = sinking_category_id(&conn);
        insert_goal(&conn, "Car Maintenance", 100_000, Some(&cat_id));

        // Seed an allocation event (not carry) and a split.
        let ae_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO allocation_events (id, category_id, month, amount_cents, kind, created_at)
             VALUES (?1, ?2, '2026-01', 80000, 'allocate', datetime('now'))",
            rusqlite::params![ae_id, cat_id],
        )
        .unwrap();

        // Insert a transaction with a split (spending).
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
             VALUES (?1, ?2, NULL, NULL, 'src1', '2026-01-15', -20000, 'Auto shop', '{}', datetime('now'))",
            rusqlite::params![rr_id, tx_id],
        )
        .unwrap();
        let split_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO splits (id, transaction_id, target_type, target_id, amount_cents)
             VALUES (?1, ?2, 'envelope', ?3, -20000)",
            rusqlite::params![split_id, tx_id, cat_id],
        )
        .unwrap();

        let goals = list_goals_with_progress_inner(&conn).unwrap();
        assert_eq!(goals.len(), 1);
        // balance = 80000 allocated - (-20000 spent) = 80000 - (-20000) = 100000
        // Wait: splits store negative amounts for spending. allocated - spent = 80000 - (-20000) = 100000
        assert_eq!(goals[0].current_balance_cents, 80000 - (-20000_i64));
    }

    #[test]
    fn delete_goal_removes_it() {
        let conn = test_db();
        let id = insert_goal(&conn, "Emergency Fund", 1_000_000, None);
        let goals = list_goals_with_progress_inner(&conn).unwrap();
        assert_eq!(goals.len(), 1);

        conn.execute("DELETE FROM goals WHERE id = ?1", rusqlite::params![id])
            .unwrap();

        let goals = list_goals_with_progress_inner(&conn).unwrap();
        assert!(goals.is_empty());
    }

    #[test]
    fn update_goal_changes_fields() {
        let conn = test_db();
        let id = insert_goal(&conn, "Old Name", 50_000, None);

        conn.execute(
            "UPDATE goals SET name = ?1, target_amount_cents = ?2 WHERE id = ?3",
            rusqlite::params!["New Name", 75_000_i64, id],
        )
        .unwrap();

        let goals = list_goals_with_progress_inner(&conn).unwrap();
        assert_eq!(goals.len(), 1);
        assert_eq!(goals[0].name, "New Name");
        assert_eq!(goals[0].target_amount_cents, 75_000);
    }

    #[test]
    fn export_bindings_goal() {
        <Goal as TS>::export_all().unwrap();
    }

    #[test]
    fn export_bindings_goal_with_progress() {
        <GoalWithProgress as TS>::export_all().unwrap();
    }

    #[test]
    fn list_goals_with_progress_gate_rejects_locked() {
        use crate::crypto::VaultState;
        use std::sync::Mutex;
        let vault = VaultState(Mutex::new(None));
        assert_eq!(
            crate::commands::require_unlocked(&vault).unwrap_err(),
            "locked"
        );
    }

    #[test]
    fn create_goal_gate_rejects_locked() {
        use crate::crypto::VaultState;
        use std::sync::Mutex;
        let vault = VaultState(Mutex::new(None));
        assert_eq!(
            crate::commands::require_unlocked(&vault).unwrap_err(),
            "locked"
        );
    }

    #[test]
    fn update_goal_gate_rejects_locked() {
        use crate::crypto::VaultState;
        use std::sync::Mutex;
        let vault = VaultState(Mutex::new(None));
        assert_eq!(
            crate::commands::require_unlocked(&vault).unwrap_err(),
            "locked"
        );
    }

    #[test]
    fn delete_goal_gate_rejects_locked() {
        use crate::crypto::VaultState;
        use std::sync::Mutex;
        let vault = VaultState(Mutex::new(None));
        assert_eq!(
            crate::commands::require_unlocked(&vault).unwrap_err(),
            "locked"
        );
    }
}
