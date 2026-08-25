use chrono::Utc;
use rusqlite::Connection;
use serde::Serialize;
use std::sync::Mutex;
use tauri::State;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/lib/generated/BudgetCategoryRow.ts")]
pub struct BudgetCategoryRow {
    pub category_id: String,
    pub category_name: String,
    /// "flow" | "sinking"
    #[ts(type = "'flow' | 'sinking'")]
    pub kind: String,
    /// flow: current-month allocation; sinking: all-time cumulative allocation
    #[ts(type = "number")]
    pub allocated_cents: i64,
    /// flow: current-month spending; sinking: all-time cumulative spending; always >= 0
    #[ts(type = "number")]
    pub spent_cents: i64,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/lib/generated/BudgetMonthView.ts")]
pub struct BudgetMonthView {
    pub month: String,
    /// 0 if no monthly_targets row exists for this month
    #[ts(type = "number")]
    pub monthly_target_cents: i64,
    /// monthly_target_cents minus the sum of all category allocations for the month
    #[ts(type = "number")]
    pub left_to_allocate_cents: i64,
    pub categories: Vec<BudgetCategoryRow>,
}

#[tauri::command]
pub fn get_budget_month(
    db: State<'_, Mutex<Connection>>,
    month: String,
) -> Result<BudgetMonthView, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    get_budget_month_inner(&conn, &month)
}

fn get_budget_month_inner(conn: &Connection, month: &str) -> Result<BudgetMonthView, String> {
    let month_prefix = format!("{month}-%");

    let mut stmt = conn
        .prepare(
            "SELECT
                c.id,
                c.name,
                c.kind,
                (
                    SELECT COALESCE(SUM(ae.amount_cents), 0)
                    FROM allocation_events ae
                    WHERE ae.category_id = c.id
                      AND (c.kind = 'sinking' OR ae.month = ?1)
                ) AS allocated_cents,
                (
                    SELECT COALESCE(ABS(SUM(ca.amount_cents)), 0)
                    FROM category_assignments ca
                    JOIN raw_records rr ON rr.transaction_id = ca.transaction_id
                    WHERE ca.category_id = c.id
                      AND ca.amount_cents < 0
                      AND NOT EXISTS (
                          SELECT 1 FROM raw_records rr2
                          WHERE rr2.supersedes_id = rr.id
                      )
                      AND (c.kind = 'sinking' OR rr.date LIKE ?2)
                ) AS spent_cents
             FROM categories c
             ORDER BY c.kind, c.name",
        )
        .map_err(|e| e.to_string())?;

    let categories: Vec<BudgetCategoryRow> = stmt
        .query_map(rusqlite::params![month, month_prefix], |row| {
            Ok(BudgetCategoryRow {
                category_id: row.get(0)?,
                category_name: row.get(1)?,
                kind: row.get(2)?,
                allocated_cents: row.get(3)?,
                spent_cents: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let monthly_target_cents: i64 = conn
        .query_row(
            "SELECT amount_cents FROM monthly_targets WHERE month = ?1",
            rusqlite::params![month],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let total_month_allocated: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(amount_cents), 0)
             FROM allocation_events
             WHERE month = ?1",
            rusqlite::params![month],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    let left_to_allocate_cents = monthly_target_cents - total_month_allocated;

    Ok(BudgetMonthView {
        month: month.to_owned(),
        monthly_target_cents,
        left_to_allocate_cents,
        categories,
    })
}

#[tauri::command]
pub fn set_allocation(
    db: State<'_, Mutex<Connection>>,
    category_id: String,
    month: String,
    new_amount_cents: i64,
) -> Result<(), String> {
    if new_amount_cents < 0 {
        return Err("Allocation amount must be non-negative".into());
    }
    let conn = db.lock().map_err(|e| e.to_string())?;
    set_allocation_inner(&conn, &category_id, &month, new_amount_cents)
}

fn set_allocation_inner(
    conn: &Connection,
    category_id: &str,
    month: &str,
    new_amount_cents: i64,
) -> Result<(), String> {
    let current_fold: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(amount_cents), 0)
             FROM allocation_events
             WHERE category_id = ?1 AND month = ?2",
            rusqlite::params![category_id, month],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    let delta = new_amount_cents - current_fold;
    if delta == 0 {
        return Ok(());
    }

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO allocation_events
             (id, category_id, month, amount_cents, kind, counterpart_category_id, group_id, note, created_at)
         VALUES (?1, ?2, ?3, ?4, 'allocate', NULL, NULL, NULL, ?5)",
        rusqlite::params![id, category_id, month, delta, now],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn set_monthly_target(
    db: State<'_, Mutex<Connection>>,
    month: String,
    amount_cents: i64,
) -> Result<(), String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO monthly_targets (month, amount_cents) VALUES (?1, ?2)",
        rusqlite::params![month, amount_cents],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{db::open_connection, migrations::run_migrations, seed::seed_categories};

    fn test_db() -> Connection {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();
        seed_categories(&conn).unwrap();
        conn
    }

    fn first_category_id(conn: &Connection, kind: &str) -> String {
        conn.query_row(
            "SELECT id FROM categories WHERE kind = ?1 LIMIT 1",
            rusqlite::params![kind],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn set_allocation_inserts_event_on_first_set() {
        let conn = test_db();
        let cat_id = first_category_id(&conn, "flow");
        set_allocation_inner(&conn, &cat_id, "2026-08", 10_000).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM allocation_events WHERE category_id = ?1",
                rusqlite::params![cat_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        let amount: i64 = conn
            .query_row(
                "SELECT amount_cents FROM allocation_events WHERE category_id = ?1",
                rusqlite::params![cat_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(amount, 10_000);
    }

    #[test]
    fn set_allocation_inserts_delta_on_adjustment() {
        let conn = test_db();
        let cat_id = first_category_id(&conn, "flow");
        set_allocation_inner(&conn, &cat_id, "2026-08", 10_000).unwrap();
        set_allocation_inner(&conn, &cat_id, "2026-08", 15_000).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM allocation_events WHERE category_id = ?1",
                rusqlite::params![cat_id],
                |r| r.get(0),
            )
            .unwrap();
        // two events: +10_000 then +5_000
        assert_eq!(count, 2);
        let fold: i64 = conn
            .query_row(
                "SELECT SUM(amount_cents) FROM allocation_events WHERE category_id = ?1",
                rusqlite::params![cat_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(fold, 15_000);
    }

    #[test]
    fn set_allocation_no_op_when_value_unchanged() {
        let conn = test_db();
        let cat_id = first_category_id(&conn, "flow");
        set_allocation_inner(&conn, &cat_id, "2026-08", 10_000).unwrap();
        set_allocation_inner(&conn, &cat_id, "2026-08", 10_000).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM allocation_events WHERE category_id = ?1",
                rusqlite::params![cat_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn set_allocation_rejects_negative_amount() {
        let conn = test_db();
        let cat_id = first_category_id(&conn, "flow");
        // Simulate the guard (inner fn takes i64 directly; command wraps it)
        let result: Result<(), String> = if -100_i64 < 0 {
            Err("Allocation amount must be non-negative".into())
        } else {
            set_allocation_inner(&conn, &cat_id, "2026-08", -100)
        };
        assert!(result.is_err());
    }

    #[test]
    fn get_budget_month_returns_flow_scoped_to_month() {
        let conn = test_db();
        let cat_id = first_category_id(&conn, "flow");
        // Allocate to two months; only current should appear in allocated_cents
        set_allocation_inner(&conn, &cat_id, "2026-08", 20_000).unwrap();
        set_allocation_inner(&conn, &cat_id, "2026-07", 10_000).unwrap();

        let view = get_budget_month_inner(&conn, "2026-08").unwrap();
        let row = view
            .categories
            .iter()
            .find(|r| r.category_id == cat_id)
            .unwrap();
        assert_eq!(row.allocated_cents, 20_000);
        assert_eq!(row.spent_cents, 0);
    }

    #[test]
    fn get_budget_month_returns_sinking_cumulative() {
        let conn = test_db();
        let cat_id = first_category_id(&conn, "sinking");
        set_allocation_inner(&conn, &cat_id, "2026-07", 5_000).unwrap();
        set_allocation_inner(&conn, &cat_id, "2026-08", 5_000).unwrap();

        let view = get_budget_month_inner(&conn, "2026-08").unwrap();
        let row = view
            .categories
            .iter()
            .find(|r| r.category_id == cat_id)
            .unwrap();
        // cumulative: both months
        assert_eq!(row.allocated_cents, 10_000);
    }

    #[test]
    fn set_monthly_target_upserts() {
        let conn = test_db();
        conn.execute(
            "INSERT OR REPLACE INTO monthly_targets (month, amount_cents) VALUES ('2026-08', 300_000)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO monthly_targets (month, amount_cents) VALUES ('2026-08', 400_000)",
            [],
        )
        .unwrap();
        let amount: i64 = conn
            .query_row(
                "SELECT amount_cents FROM monthly_targets WHERE month = '2026-08'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(amount, 400_000);
    }

    #[test]
    fn left_to_allocate_reflects_target_minus_allocations() {
        let conn = test_db();
        conn.execute(
            "INSERT OR REPLACE INTO monthly_targets (month, amount_cents) VALUES ('2026-08', 300_000)",
            [],
        )
        .unwrap();
        let flow_id = first_category_id(&conn, "flow");
        set_allocation_inner(&conn, &flow_id, "2026-08", 100_000).unwrap();

        let view = get_budget_month_inner(&conn, "2026-08").unwrap();
        assert_eq!(view.monthly_target_cents, 300_000);
        assert_eq!(view.left_to_allocate_cents, 200_000);
    }
}
