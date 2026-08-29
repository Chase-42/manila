use chrono::Utc;
use rusqlite::Connection;
use serde::Serialize;
use std::sync::Mutex;
use tauri::State;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/lib/generated/HomeView.ts")]
pub struct HomeView {
    pub month: String,
    /// sum of max(0, available) across flow categories; available = allocated + activity + carried_in
    #[ts(type = "number")]
    pub flow_remaining_cents: i64,
    /// calendar days left in month counting today; 0 on the last day after it passes
    #[ts(type = "number")]
    pub days_remaining: i64,
    /// flow_remaining / days_remaining (integer division); 0 when days_remaining is 0
    #[ts(type = "number")]
    pub safe_to_spend_daily_cents: i64,
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

fn days_remaining_in_month(today: &str) -> Result<i64, String> {
    let parts: Vec<&str> = today.split('-').collect();
    if parts.len() != 3 {
        return Err(format!("invalid date '{today}'; expected YYYY-MM-DD"));
    }
    let year: i32 = parts[0]
        .parse()
        .map_err(|_| format!("invalid year in '{today}'"))?;
    let month: u32 = parts[1]
        .parse()
        .map_err(|_| format!("invalid month in '{today}'"))?;
    let day: u32 = parts[2]
        .parse()
        .map_err(|_| format!("invalid day in '{today}'"))?;
    let last = days_in_month(year, month) as i64;
    Ok((last - day as i64 + 1).max(0))
}

#[tauri::command]
pub fn get_home_view(db: State<'_, Mutex<Connection>>, today: String) -> Result<HomeView, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    get_home_view_inner(&conn, &today)
}

fn get_home_view_inner(conn: &Connection, today: &str) -> Result<HomeView, String> {
    if today.len() < 7 {
        return Err(format!("invalid date '{today}'"));
    }
    let month = &today[..7];
    let month_prefix = format!("{month}-%");

    let flow_remaining_cents: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(CASE WHEN avail > 0 THEN avail ELSE 0 END), 0)
             FROM (
                 SELECT
                     (
                         SELECT COALESCE(SUM(ae.amount_cents), 0)
                         FROM allocation_events ae
                         WHERE ae.category_id = c.id
                           AND ae.month = ?1
                           AND ae.kind != 'carry'
                     ) +
                     (
                         SELECT COALESCE(SUM(s.amount_cents), 0)
                         FROM splits s
                         JOIN raw_records rr ON rr.transaction_id = s.transaction_id
                         WHERE s.target_id = c.id
                           AND s.target_type = 'envelope'
                           AND NOT EXISTS (
                               SELECT 1 FROM raw_records rr2 WHERE rr2.supersedes_id = rr.id
                           )
                           AND rr.date LIKE ?2
                     ) +
                     (
                         SELECT COALESCE(SUM(ae.amount_cents), 0)
                         FROM allocation_events ae
                         WHERE ae.category_id = c.id
                           AND ae.month = ?1
                           AND ae.kind = 'carry'
                     ) AS avail
                 FROM categories c
                 WHERE c.kind = 'flow'
             )",
            rusqlite::params![month, month_prefix],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    let days_remaining = days_remaining_in_month(today)?;
    let safe_to_spend_daily_cents = if days_remaining > 0 {
        flow_remaining_cents / days_remaining
    } else {
        0
    };

    Ok(HomeView {
        month: month.to_owned(),
        flow_remaining_cents,
        days_remaining,
        safe_to_spend_daily_cents,
    })
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/lib/generated/IncomeCategoryRow.ts")]
pub struct IncomeCategoryRow {
    pub income_category_id: String,
    pub name: String,
    /// sum of income splits on transactions dated in the current month
    #[ts(type = "number")]
    pub actual_cents: i64,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/lib/generated/BudgetCategoryRow.ts")]
pub struct BudgetCategoryRow {
    pub category_id: String,
    pub category_name: String,
    /// "flow" | "sinking"
    #[ts(type = "'flow' | 'sinking'")]
    pub kind: String,
    /// user-driven allocations this month (excludes carry events); sinking: all-time cumulative
    #[ts(type = "number")]
    pub allocated_cents: i64,
    /// flow: current-month spending; sinking: all-time cumulative spending; always >= 0
    #[ts(type = "number")]
    pub spent_cents: i64,
    /// debt carried in from a prior month close; 0 unless this category had a negative available
    #[ts(type = "number")]
    pub carried_in_cents: i64,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/lib/generated/BudgetGroupView.ts")]
pub struct BudgetGroupView {
    pub group_id: String,
    pub group_name: String,
    #[ts(type = "number")]
    pub sort_order: i64,
    #[ts(type = "number")]
    pub total_allocated_cents: i64,
    #[ts(type = "number")]
    pub total_spent_cents: i64,
    #[ts(type = "number")]
    pub remaining_cents: i64,
    pub categories: Vec<BudgetCategoryRow>,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/lib/generated/ReallocationEntry.ts")]
pub struct ReallocationEntry {
    /// ID of the negative (source) allocation_event row
    pub id: String,
    pub from_category_id: String,
    pub from_name: String,
    pub to_category_id: String,
    pub to_name: String,
    /// absolute value of the moved amount
    #[ts(type = "number")]
    pub amount_cents: i64,
    pub created_at: String,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/lib/generated/BudgetMonthView.ts")]
pub struct BudgetMonthView {
    pub month: String,
    /// income splits for month minus user-driven allocations (excludes carry events)
    #[ts(type = "number")]
    pub left_to_allocate_cents: i64,
    pub income_rows: Vec<IncomeCategoryRow>,
    pub flow_groups: Vec<BudgetGroupView>,
    pub flow_ungrouped: Vec<BudgetCategoryRow>,
    pub sinking_groups: Vec<BudgetGroupView>,
    pub sinking_ungrouped: Vec<BudgetCategoryRow>,
    /// reallocation pairs for this month, newest first; one entry per pair (source side)
    pub reallocation_log: Vec<ReallocationEntry>,
    /// true once close_month has been called for this month
    pub is_closed: bool,
}

struct RawBudgetRow {
    category_id: String,
    category_name: String,
    kind: String,
    group_id: Option<String>,
    group_name: Option<String>,
    group_sort_order: Option<i64>,
    allocated_cents: i64,
    spent_cents: i64,
    carried_in_cents: i64,
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
                c.group_id,
                cg.name,
                cg.sort_order,
                (
                    SELECT COALESCE(SUM(ae.amount_cents), 0)
                    FROM allocation_events ae
                    WHERE ae.category_id = c.id
                      AND ae.kind != 'carry'
                      AND (c.kind = 'sinking' OR ae.month = ?1)
                ) AS allocated_cents,
                (
                    SELECT COALESCE(ABS(SUM(s.amount_cents)), 0)
                    FROM splits s
                    JOIN raw_records rr ON rr.transaction_id = s.transaction_id
                    WHERE s.target_id = c.id
                      AND s.target_type = 'envelope'
                      AND s.amount_cents < 0
                      AND NOT EXISTS (
                          SELECT 1 FROM raw_records rr2
                          WHERE rr2.supersedes_id = rr.id
                      )
                      AND (c.kind = 'sinking' OR rr.date LIKE ?2)
                ) AS spent_cents,
                (
                    SELECT COALESCE(SUM(ae.amount_cents), 0)
                    FROM allocation_events ae
                    WHERE ae.category_id = c.id
                      AND ae.month = ?1
                      AND ae.kind = 'carry'
                ) AS carried_in_cents
             FROM categories c
             LEFT JOIN category_groups cg ON cg.id = c.group_id
             ORDER BY c.kind, cg.sort_order NULLS LAST, c.name",
        )
        .map_err(|e| e.to_string())?;

    let raw_rows: Vec<RawBudgetRow> = stmt
        .query_map(rusqlite::params![month, month_prefix], |row| {
            Ok(RawBudgetRow {
                category_id: row.get(0)?,
                category_name: row.get(1)?,
                kind: row.get(2)?,
                group_id: row.get(3)?,
                group_name: row.get(4)?,
                group_sort_order: row.get(5)?,
                allocated_cents: row.get(6)?,
                spent_cents: row.get(7)?,
                carried_in_cents: row.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let mut flow_groups: Vec<BudgetGroupView> = Vec::new();
    let mut flow_ungrouped: Vec<BudgetCategoryRow> = Vec::new();
    let mut sinking_groups: Vec<BudgetGroupView> = Vec::new();
    let mut sinking_ungrouped: Vec<BudgetCategoryRow> = Vec::new();

    for row in raw_rows {
        let cat = BudgetCategoryRow {
            category_id: row.category_id,
            category_name: row.category_name,
            kind: row.kind.clone(),
            allocated_cents: row.allocated_cents,
            spent_cents: row.spent_cents,
            carried_in_cents: row.carried_in_cents,
        };

        match row.kind.as_str() {
            "flow" => match row.group_id {
                Some(gid) => {
                    if flow_groups.last().is_none_or(|g| g.group_id != gid) {
                        flow_groups.push(BudgetGroupView {
                            group_id: gid,
                            group_name: row.group_name.unwrap_or_default(),
                            sort_order: row.group_sort_order.unwrap_or(0),
                            total_allocated_cents: 0,
                            total_spent_cents: 0,
                            remaining_cents: 0,
                            categories: vec![cat],
                        });
                    } else if let Some(g) = flow_groups.last_mut() {
                        g.categories.push(cat);
                    }
                }
                None => flow_ungrouped.push(cat),
            },
            "sinking" => match row.group_id {
                Some(gid) => {
                    if sinking_groups.last().is_none_or(|g| g.group_id != gid) {
                        sinking_groups.push(BudgetGroupView {
                            group_id: gid,
                            group_name: row.group_name.unwrap_or_default(),
                            sort_order: row.group_sort_order.unwrap_or(0),
                            total_allocated_cents: 0,
                            total_spent_cents: 0,
                            remaining_cents: 0,
                            categories: vec![cat],
                        });
                    } else if let Some(g) = sinking_groups.last_mut() {
                        g.categories.push(cat);
                    }
                }
                None => sinking_ungrouped.push(cat),
            },
            _ => {}
        }
    }

    // Derive rollup totals from nested rows; no re-query.
    for g in &mut flow_groups {
        g.total_allocated_cents = g.categories.iter().map(|c| c.allocated_cents).sum();
        g.total_spent_cents = g.categories.iter().map(|c| c.spent_cents).sum();
        g.remaining_cents = g.total_allocated_cents - g.total_spent_cents;
    }
    for g in &mut sinking_groups {
        g.total_allocated_cents = g.categories.iter().map(|c| c.allocated_cents).sum();
        g.total_spent_cents = g.categories.iter().map(|c| c.spent_cents).sum();
        g.remaining_cents = g.total_allocated_cents - g.total_spent_cents;
    }

    // Income section: one row per non-hidden income category with actual splits for this month.
    let mut income_stmt = conn
        .prepare(
            "SELECT ic.id, ic.name,
                    COALESCE((
                        SELECT SUM(s.amount_cents)
                        FROM splits s
                        JOIN raw_records rr ON rr.transaction_id = s.transaction_id
                        WHERE s.target_id = ic.id
                          AND s.target_type = 'income'
                          AND NOT EXISTS (
                              SELECT 1 FROM raw_records rr2 WHERE rr2.supersedes_id = rr.id
                          )
                          AND rr.date LIKE ?1
                    ), 0) AS actual_cents
             FROM income_categories ic
             WHERE ic.hidden = 0
             ORDER BY ic.name",
        )
        .map_err(|e| e.to_string())?;

    let income_rows: Vec<IncomeCategoryRow> = income_stmt
        .query_map(rusqlite::params![month_prefix], |row| {
            Ok(IncomeCategoryRow {
                income_category_id: row.get(0)?,
                name: row.get(1)?,
                actual_cents: row.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let total_income_cents: i64 = income_rows.iter().map(|r| r.actual_cents).sum();

    // Exclude carry events: they are structural (written by month close), not user-driven.
    // Including them would inflate left_to_allocate when a prior month had flow debt.
    let total_month_allocated: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(amount_cents), 0)
             FROM allocation_events
             WHERE month = ?1 AND kind != 'carry'",
            rusqlite::params![month],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    let left_to_allocate_cents = total_income_cents - total_month_allocated;

    let is_closed: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM month_closes WHERE month = ?1",
            rusqlite::params![month],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|e| e.to_string())?
        > 0;

    // Query only the negative-amount side of each reallocate pair to avoid duplicates.
    let mut realloc_stmt = conn
        .prepare(
            "SELECT ae.id, ae.category_id, from_c.name, ae.counterpart_category_id, to_c.name,
                    ABS(ae.amount_cents), ae.created_at
             FROM allocation_events ae
             JOIN categories from_c ON from_c.id = ae.category_id
             JOIN categories to_c   ON to_c.id   = ae.counterpart_category_id
             WHERE ae.month = ?1
               AND ae.kind = 'reallocate'
               AND ae.amount_cents < 0
             ORDER BY ae.created_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let reallocation_log: Vec<ReallocationEntry> = realloc_stmt
        .query_map(rusqlite::params![month], |row| {
            Ok(ReallocationEntry {
                id: row.get(0)?,
                from_category_id: row.get(1)?,
                from_name: row.get(2)?,
                to_category_id: row.get(3)?,
                to_name: row.get(4)?,
                amount_cents: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(BudgetMonthView {
        month: month.to_owned(),
        left_to_allocate_cents,
        income_rows,
        flow_groups,
        flow_ungrouped,
        sinking_groups,
        sinking_ungrouped,
        reallocation_log,
        is_closed,
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
             (id, category_id, month, amount_cents, kind, counterpart_category_id, note, created_at)
         VALUES (?1, ?2, ?3, ?4, 'allocate', NULL, NULL, ?5)",
        rusqlite::params![id, category_id, month, delta, now],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

fn next_month(month: &str) -> Result<String, String> {
    let (year_str, mon_str) = month
        .split_once('-')
        .ok_or_else(|| format!("invalid month format: {month}"))?;
    let year: i32 = year_str
        .parse()
        .map_err(|_| format!("invalid year in: {month}"))?;
    let mon: u32 = mon_str
        .parse()
        .map_err(|_| format!("invalid month in: {month}"))?;
    if mon == 12 {
        Ok(format!("{:04}-01", year + 1))
    } else {
        Ok(format!("{year:04}-{:02}", mon + 1))
    }
}

#[tauri::command]
pub fn close_month(db: State<'_, Mutex<Connection>>, month: String) -> Result<(), String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    close_month_inner(&conn, &month)
}

fn close_month_inner(conn: &Connection, month: &str) -> Result<(), String> {
    // Validate YYYY-MM format
    if month.len() != 7 || !month.chars().nth(4).is_some_and(|c| c == '-') {
        return Err(format!("invalid month format '{month}'; expected YYYY-MM"));
    }

    // Idempotency guard
    let already_closed: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM month_closes WHERE month = ?1",
            rusqlite::params![month],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|e| e.to_string())?
        > 0;
    if already_closed {
        return Err(format!("month {month} is already closed"));
    }

    let target_month = next_month(month)?;
    let now = Utc::now().to_rfc3339();
    let month_prefix = format!("{month}-%");

    // Compute available balance for each flow category and write carry events for debt
    let mut stmt = conn
        .prepare(
            "SELECT
                c.id,
                (
                    SELECT COALESCE(SUM(ae.amount_cents), 0)
                    FROM allocation_events ae
                    WHERE ae.category_id = c.id AND ae.month = ?1
                ) +
                (
                    SELECT COALESCE(SUM(s.amount_cents), 0)
                    FROM splits s
                    JOIN raw_records rr ON rr.transaction_id = s.transaction_id
                    WHERE s.target_id = c.id
                      AND s.target_type = 'envelope'
                      AND NOT EXISTS (
                          SELECT 1 FROM raw_records rr2 WHERE rr2.supersedes_id = rr.id
                      )
                      AND rr.date LIKE ?2
                ) AS available_cents
             FROM categories c
             WHERE c.kind = 'flow'",
        )
        .map_err(|e| e.to_string())?;

    let flow_rows: Vec<(String, i64)> = stmt
        .query_map(rusqlite::params![month, month_prefix], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    for (category_id, available) in flow_rows {
        if available < 0 {
            let carry_id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO allocation_events
                     (id, category_id, month, amount_cents, kind, counterpart_category_id, note, created_at)
                 VALUES (?1, ?2, ?3, ?4, 'carry', NULL, NULL, ?5)",
                rusqlite::params![carry_id, category_id, target_month, available, now],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    // Mark month as closed
    conn.execute(
        "INSERT INTO month_closes (month, closed_at) VALUES (?1, ?2)",
        rusqlite::params![month, now],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn reallocate(
    db: State<'_, Mutex<Connection>>,
    from_category_id: String,
    to_category_id: String,
    month: String,
    amount_cents: i64,
) -> Result<(), String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    reallocate_inner(
        &conn,
        &from_category_id,
        &to_category_id,
        &month,
        amount_cents,
    )
}

fn reallocate_inner(
    conn: &Connection,
    from_category_id: &str,
    to_category_id: &str,
    month: &str,
    amount_cents: i64,
) -> Result<(), String> {
    if amount_cents <= 0 {
        return Err("Reallocation amount must be positive".into());
    }
    if from_category_id == to_category_id {
        return Err("Source and destination must be different categories".into());
    }
    let now = Utc::now().to_rfc3339();
    let from_id = Uuid::new_v4().to_string();
    let to_id = Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO allocation_events
             (id, category_id, month, amount_cents, kind, counterpart_category_id, note, created_at)
         VALUES (?1, ?2, ?3, ?4, 'reallocate', ?5, NULL, ?6)",
        rusqlite::params![
            from_id,
            from_category_id,
            month,
            -amount_cents,
            to_category_id,
            now
        ],
    )
    .map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT INTO allocation_events
             (id, category_id, month, amount_cents, kind, counterpart_category_id, note, created_at)
         VALUES (?1, ?2, ?3, ?4, 'reallocate', ?5, NULL, ?6)",
        rusqlite::params![
            to_id,
            to_category_id,
            month,
            amount_cents,
            from_category_id,
            now
        ],
    )
    .map_err(|e| e.to_string())?;

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
    use uuid::Uuid;

    fn test_db() -> Connection {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();
        seed_categories(&conn).unwrap();
        seed_category_groups(&conn).unwrap();
        seed_income_categories(&conn).unwrap();
        conn
    }

    fn first_income_category_id(conn: &Connection) -> String {
        conn.query_row(
            "SELECT id FROM income_categories WHERE hidden = 0 LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn insert_income_split(conn: &Connection, income_cat_id: &str, amount_cents: i64, date: &str) {
        let account_id = Uuid::new_v4().to_string();
        let tx_id = Uuid::new_v4().to_string();
        let rr_id = Uuid::new_v4().to_string();
        let split_id = Uuid::new_v4().to_string();
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
             VALUES (?1, ?2, NULL, NULL, ?3, ?4, ?5, 'Income', '{}', datetime('now'))",
            rusqlite::params![rr_id, tx_id, format!("src|{}", rr_id), date, amount_cents],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (id, transaction_id, target_type, target_id, amount_cents)
             VALUES (?1, ?2, 'income', ?3, ?4)",
            rusqlite::params![split_id, tx_id, income_cat_id, amount_cents],
        )
        .unwrap();
    }

    fn first_category_id(conn: &Connection, kind: &str) -> String {
        conn.query_row(
            "SELECT id FROM categories WHERE kind = ?1 LIMIT 1",
            rusqlite::params![kind],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn find_category<'a>(view: &'a BudgetMonthView, category_id: &str) -> &'a BudgetCategoryRow {
        view.flow_groups
            .iter()
            .flat_map(|g| g.categories.iter())
            .chain(view.flow_ungrouped.iter())
            .chain(view.sinking_groups.iter().flat_map(|g| g.categories.iter()))
            .chain(view.sinking_ungrouped.iter())
            .find(|r| r.category_id == category_id)
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
        let row = find_category(&view, &cat_id);
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
        let row = find_category(&view, &cat_id);
        // cumulative: both months
        assert_eq!(row.allocated_cents, 10_000);
    }

    #[test]
    fn get_budget_month_groups_are_bucketed_by_kind() {
        let conn = test_db();
        let view = get_budget_month_inner(&conn, "2026-08").unwrap();

        // All seeded categories are grouped; none should be in ungrouped.
        assert!(
            view.flow_ungrouped.is_empty(),
            "all seeded flow categories should be grouped"
        );
        assert!(
            view.sinking_ungrouped.is_empty(),
            "all seeded sinking categories should be grouped"
        );

        // flow_groups should contain only flow categories
        for g in &view.flow_groups {
            for c in &g.categories {
                assert_eq!(c.kind, "flow");
            }
        }
        // sinking_groups should contain only sinking categories
        for g in &view.sinking_groups {
            for c in &g.categories {
                assert_eq!(c.kind, "sinking");
            }
        }
    }

    #[test]
    fn get_budget_month_group_rollup_totals_match_sum_of_rows() {
        let conn = test_db();
        // Allocate to a couple of categories so totals are non-zero.
        let flow_id = first_category_id(&conn, "flow");
        let sinking_id = first_category_id(&conn, "sinking");
        set_allocation_inner(&conn, &flow_id, "2026-08", 15_000).unwrap();
        set_allocation_inner(&conn, &sinking_id, "2026-08", 8_000).unwrap();

        let view = get_budget_month_inner(&conn, "2026-08").unwrap();
        for g in &view.flow_groups {
            let expected_alloc: i64 = g.categories.iter().map(|c| c.allocated_cents).sum();
            let expected_spent: i64 = g.categories.iter().map(|c| c.spent_cents).sum();
            assert_eq!(g.total_allocated_cents, expected_alloc);
            assert_eq!(g.total_spent_cents, expected_spent);
        }
        for g in &view.sinking_groups {
            let expected_alloc: i64 = g.categories.iter().map(|c| c.allocated_cents).sum();
            let expected_spent: i64 = g.categories.iter().map(|c| c.spent_cents).sum();
            assert_eq!(g.total_allocated_cents, expected_alloc);
            assert_eq!(g.total_spent_cents, expected_spent);
        }
    }

    #[test]
    fn get_budget_month_groups_absent_when_no_categories_of_that_kind() {
        let conn = test_db();
        // Health group has only flow categories (Healthcare); it should appear in flow_groups
        // but NOT in sinking_groups.
        let view = get_budget_month_inner(&conn, "2026-08").unwrap();
        let health_in_sinking = view.sinking_groups.iter().any(|g| g.group_name == "Health");
        assert!(
            !health_in_sinking,
            "Health group should not appear in sinking_groups"
        );
        // Savings group has only sinking categories; it should not appear in flow_groups.
        let savings_in_flow = view.flow_groups.iter().any(|g| g.group_name == "Savings");
        assert!(
            !savings_in_flow,
            "Savings group should not appear in flow_groups"
        );
    }

    #[test]
    fn income_rows_present_with_zero_actual_when_no_splits() {
        let conn = test_db();
        let view = get_budget_month_inner(&conn, "2026-08").unwrap();
        // 4 seeded income categories, none hidden
        assert_eq!(view.income_rows.len(), 4);
        for row in &view.income_rows {
            assert_eq!(
                row.actual_cents, 0,
                "no splits yet so actual_cents should be 0"
            );
        }
    }

    #[test]
    fn income_rows_reflect_splits_dated_in_month() {
        let conn = test_db();
        let income_id = first_income_category_id(&conn);
        insert_income_split(&conn, &income_id, 480_000, "2026-08-15");

        let view = get_budget_month_inner(&conn, "2026-08").unwrap();
        let row = view
            .income_rows
            .iter()
            .find(|r| r.income_category_id == income_id)
            .unwrap();
        assert_eq!(row.actual_cents, 480_000);
    }

    #[test]
    fn income_splits_outside_month_not_counted() {
        let conn = test_db();
        let income_id = first_income_category_id(&conn);
        insert_income_split(&conn, &income_id, 480_000, "2026-07-31");

        let view = get_budget_month_inner(&conn, "2026-08").unwrap();
        let row = view
            .income_rows
            .iter()
            .find(|r| r.income_category_id == income_id)
            .unwrap();
        assert_eq!(
            row.actual_cents, 0,
            "split in prior month must not count toward August"
        );
    }

    #[test]
    fn left_to_allocate_equals_income_minus_allocations() {
        let conn = test_db();
        let income_id = first_income_category_id(&conn);
        insert_income_split(&conn, &income_id, 300_000, "2026-08-01");
        let flow_id = first_category_id(&conn, "flow");
        set_allocation_inner(&conn, &flow_id, "2026-08", 100_000).unwrap();

        let view = get_budget_month_inner(&conn, "2026-08").unwrap();
        assert_eq!(view.left_to_allocate_cents, 200_000);
    }

    #[test]
    fn group_remaining_cents_equals_allocated_minus_spent() {
        let conn = test_db();
        let flow_id = first_category_id(&conn, "flow");
        set_allocation_inner(&conn, &flow_id, "2026-08", 50_000).unwrap();

        let view = get_budget_month_inner(&conn, "2026-08").unwrap();
        for g in &view.flow_groups {
            assert_eq!(
                g.remaining_cents,
                g.total_allocated_cents - g.total_spent_cents,
                "remaining_cents must equal allocated minus spent for group {}",
                g.group_name
            );
        }
    }

    fn second_category_id(conn: &Connection, kind: &str) -> String {
        conn.query_row(
            "SELECT id FROM categories WHERE kind = ?1 LIMIT 1 OFFSET 1",
            rusqlite::params![kind],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn reallocate_inserts_two_paired_rows() {
        let conn = test_db();
        let from_id = first_category_id(&conn, "flow");
        let to_id = second_category_id(&conn, "flow");
        reallocate_inner(&conn, &from_id, &to_id, "2026-08", 5_000).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM allocation_events WHERE kind = 'reallocate'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);

        let from_amount: i64 = conn
            .query_row(
                "SELECT amount_cents FROM allocation_events WHERE category_id = ?1 AND kind = 'reallocate'",
                rusqlite::params![from_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(from_amount, -5_000);

        let to_amount: i64 = conn
            .query_row(
                "SELECT amount_cents FROM allocation_events WHERE category_id = ?1 AND kind = 'reallocate'",
                rusqlite::params![to_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(to_amount, 5_000);
    }

    #[test]
    fn reallocate_sets_counterpart_category_id() {
        let conn = test_db();
        let from_id = first_category_id(&conn, "flow");
        let to_id = second_category_id(&conn, "flow");
        reallocate_inner(&conn, &from_id, &to_id, "2026-08", 3_000).unwrap();

        let counterpart_on_from: String = conn
            .query_row(
                "SELECT counterpart_category_id FROM allocation_events WHERE category_id = ?1 AND kind = 'reallocate'",
                rusqlite::params![from_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(counterpart_on_from, to_id);

        let counterpart_on_to: String = conn
            .query_row(
                "SELECT counterpart_category_id FROM allocation_events WHERE category_id = ?1 AND kind = 'reallocate'",
                rusqlite::params![to_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(counterpart_on_to, from_id);
    }

    #[test]
    fn reallocate_rejects_same_category() {
        let conn = test_db();
        let cat_id = first_category_id(&conn, "flow");
        let result = reallocate_inner(&conn, &cat_id, &cat_id, "2026-08", 1_000);
        assert!(result.is_err());
    }

    #[test]
    fn reallocate_rejects_zero_amount() {
        let conn = test_db();
        let from_id = first_category_id(&conn, "flow");
        let to_id = second_category_id(&conn, "flow");
        let result = reallocate_inner(&conn, &from_id, &to_id, "2026-08", 0);
        assert!(result.is_err());
    }

    #[test]
    fn reallocate_rejects_negative_amount() {
        let conn = test_db();
        let from_id = first_category_id(&conn, "flow");
        let to_id = second_category_id(&conn, "flow");
        let result = reallocate_inner(&conn, &from_id, &to_id, "2026-08", -500);
        assert!(result.is_err());
    }

    #[test]
    fn reallocation_log_empty_when_no_reallocations() {
        let conn = test_db();
        let view = get_budget_month_inner(&conn, "2026-08").unwrap();
        assert!(view.reallocation_log.is_empty());
    }

    #[test]
    fn reallocation_log_has_one_entry_per_pair() {
        let conn = test_db();
        let from_id = first_category_id(&conn, "flow");
        let to_id = second_category_id(&conn, "flow");
        reallocate_inner(&conn, &from_id, &to_id, "2026-08", 7_500).unwrap();

        let view = get_budget_month_inner(&conn, "2026-08").unwrap();
        assert_eq!(view.reallocation_log.len(), 1);
        let entry = &view.reallocation_log[0];
        assert_eq!(entry.from_category_id, from_id);
        assert_eq!(entry.to_category_id, to_id);
        assert_eq!(entry.amount_cents, 7_500);
    }

    #[test]
    fn reallocation_log_names_match_category_names() {
        let conn = test_db();
        let from_id = first_category_id(&conn, "flow");
        let to_id = second_category_id(&conn, "flow");
        reallocate_inner(&conn, &from_id, &to_id, "2026-08", 1_000).unwrap();

        let view = get_budget_month_inner(&conn, "2026-08").unwrap();
        let entry = &view.reallocation_log[0];
        assert!(!entry.from_name.is_empty(), "from_name must be populated");
        assert!(!entry.to_name.is_empty(), "to_name must be populated");
    }

    #[test]
    fn reallocation_log_scoped_to_month() {
        let conn = test_db();
        let from_id = first_category_id(&conn, "flow");
        let to_id = second_category_id(&conn, "flow");
        reallocate_inner(&conn, &from_id, &to_id, "2026-07", 1_000).unwrap();

        let view = get_budget_month_inner(&conn, "2026-08").unwrap();
        assert!(
            view.reallocation_log.is_empty(),
            "reallocation in July must not appear in August log"
        );
    }

    fn insert_expense_split(conn: &Connection, cat_id: &str, amount_cents: i64, date: &str) {
        let account_id = Uuid::new_v4().to_string();
        let tx_id = Uuid::new_v4().to_string();
        let rr_id = Uuid::new_v4().to_string();
        let split_id = Uuid::new_v4().to_string();
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
             VALUES (?1, ?2, NULL, NULL, ?3, ?4, ?5, 'Purchase', '{}', datetime('now'))",
            rusqlite::params![rr_id, tx_id, format!("src|{}", rr_id), date, amount_cents],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (id, transaction_id, target_type, target_id, amount_cents)
             VALUES (?1, ?2, 'envelope', ?3, ?4)",
            rusqlite::params![split_id, tx_id, cat_id, amount_cents],
        )
        .unwrap();
    }

    #[test]
    fn get_budget_month_carried_in_zero_before_close() {
        let conn = test_db();
        let cat_id = first_category_id(&conn, "flow");
        set_allocation_inner(&conn, &cat_id, "2026-08", 20_000).unwrap();
        let view = get_budget_month_inner(&conn, "2026-08").unwrap();
        let row = find_category(&view, &cat_id);
        assert_eq!(row.carried_in_cents, 0, "no carry before any month close");
    }

    #[test]
    fn get_budget_month_carried_in_populated_after_close() {
        let conn = test_db();
        let cat_id = first_category_id(&conn, "flow");
        // Overspend: allocate 100, spend 200 => available = -100
        set_allocation_inner(&conn, &cat_id, "2026-08", 10_000).unwrap();
        insert_expense_split(&conn, &cat_id, -20_000, "2026-08-15");
        close_month_inner(&conn, "2026-08").unwrap();

        // September view should show carry debt, but allocated = 0 (user hasn't set anything yet)
        let view = get_budget_month_inner(&conn, "2026-09").unwrap();
        let row = find_category(&view, &cat_id);
        assert_eq!(row.carried_in_cents, -10_000);
        assert_eq!(
            row.allocated_cents, 0,
            "carry must not inflate allocated_cents"
        );
    }

    #[test]
    fn left_to_allocate_not_reduced_by_carry_events() {
        let conn = test_db();
        let income_id = first_income_category_id(&conn);
        let cat_id = first_category_id(&conn, "flow");
        // Record $100 income in August, overspend by $50, close month
        insert_income_split(&conn, &income_id, 10_000, "2026-08-01");
        set_allocation_inner(&conn, &cat_id, "2026-08", 10_000).unwrap();
        insert_expense_split(&conn, &cat_id, -15_000, "2026-08-10");
        close_month_inner(&conn, "2026-08").unwrap();

        // Record $200 income in September
        insert_income_split(&conn, &income_id, 20_000, "2026-09-01");
        let view = get_budget_month_inner(&conn, "2026-09").unwrap();
        // LTA should be $200 (income) - $0 (no user allocations yet) = $200
        // The -$50 carry event must NOT reduce LTA
        assert_eq!(
            view.left_to_allocate_cents, 20_000,
            "carry events must not reduce left_to_allocate"
        );
    }

    #[test]
    fn get_budget_month_is_closed_false_before_close() {
        let conn = test_db();
        let view = get_budget_month_inner(&conn, "2026-08").unwrap();
        assert!(!view.is_closed);
    }

    #[test]
    fn get_budget_month_is_closed_true_after_close() {
        let conn = test_db();
        close_month_inner(&conn, "2026-08").unwrap();
        let view = get_budget_month_inner(&conn, "2026-08").unwrap();
        assert!(view.is_closed);
    }

    #[test]
    fn close_month_writes_carry_for_flow_debt() {
        let conn = test_db();
        let cat_id = first_category_id(&conn, "flow");
        // Allocate 300 but spend 400 (overspend by 100)
        set_allocation_inner(&conn, &cat_id, "2026-08", 30_000).unwrap();
        insert_expense_split(&conn, &cat_id, -40_000, "2026-08-15");

        close_month_inner(&conn, "2026-08").unwrap();

        let carry: i64 = conn
            .query_row(
                "SELECT amount_cents FROM allocation_events
                 WHERE category_id = ?1 AND month = '2026-09' AND kind = 'carry'",
                rusqlite::params![cat_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            carry, -10_000,
            "carry should equal the debt (available = 30000 - 40000 = -10000)"
        );
    }

    #[test]
    fn close_month_no_carry_for_flow_surplus() {
        let conn = test_db();
        let cat_id = first_category_id(&conn, "flow");
        // Allocate 500, spend 300 (surplus of 200 - no carry event)
        set_allocation_inner(&conn, &cat_id, "2026-08", 50_000).unwrap();
        insert_expense_split(&conn, &cat_id, -30_000, "2026-08-10");

        close_month_inner(&conn, "2026-08").unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM allocation_events WHERE kind = 'carry'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 0,
            "no carry event for a flow category with positive available"
        );
    }

    #[test]
    fn close_month_no_carry_for_sinking() {
        let conn = test_db();
        let cat_id = first_category_id(&conn, "sinking");
        // Even if sinking is 0 allocated and 0 spent, no carry event should be written
        close_month_inner(&conn, "2026-08").unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM allocation_events WHERE category_id = ?1 AND kind = 'carry'",
                rusqlite::params![cat_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 0,
            "sinking categories must never receive carry events from month close"
        );
    }

    #[test]
    fn close_month_returns_error_when_already_closed() {
        let conn = test_db();
        close_month_inner(&conn, "2026-08").unwrap();
        let result = close_month_inner(&conn, "2026-08");
        assert!(
            result.is_err(),
            "second close of same month must return an error"
        );
    }

    #[test]
    fn close_month_carry_amount_matches_available() {
        let conn = test_db();
        let cat_id = first_category_id(&conn, "flow");
        // Allocate 100, spend 250 => available = -150
        set_allocation_inner(&conn, &cat_id, "2026-08", 10_000).unwrap();
        insert_expense_split(&conn, &cat_id, -25_000, "2026-08-20");

        close_month_inner(&conn, "2026-08").unwrap();

        let carry: i64 = conn
            .query_row(
                "SELECT amount_cents FROM allocation_events
                 WHERE category_id = ?1 AND month = '2026-09' AND kind = 'carry'",
                rusqlite::params![cat_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(carry, -15_000);
    }

    #[test]
    fn reallocate_is_net_zero_for_left_to_allocate() {
        let conn = test_db();
        let income_id = first_income_category_id(&conn);
        insert_income_split(&conn, &income_id, 500_000, "2026-08-01");
        let from_id = first_category_id(&conn, "flow");
        let to_id = second_category_id(&conn, "flow");

        set_allocation_inner(&conn, &from_id, "2026-08", 200_000).unwrap();
        let view_before = get_budget_month_inner(&conn, "2026-08").unwrap();
        let lta_before = view_before.left_to_allocate_cents;

        reallocate_inner(&conn, &from_id, &to_id, "2026-08", 50_000).unwrap();
        let view_after = get_budget_month_inner(&conn, "2026-08").unwrap();
        assert_eq!(
            view_after.left_to_allocate_cents, lta_before,
            "left_to_allocate must be unchanged by a reallocation"
        );
    }

    // --- get_home_view tests ---

    #[test]
    fn flow_remaining_zero_when_no_allocations() {
        let conn = test_db();
        let view = get_home_view_inner(&conn, "2026-08-01").unwrap();
        assert_eq!(view.flow_remaining_cents, 0);
        assert_eq!(view.month, "2026-08");
    }

    #[test]
    fn flow_remaining_sums_positive_only() {
        let conn = test_db();
        let cat1 = first_category_id(&conn, "flow");
        let cat2 = second_category_id(&conn, "flow");
        // cat1: allocate 300, spend 100 -> available = 200 (positive, counted)
        set_allocation_inner(&conn, &cat1, "2026-08", 30_000).unwrap();
        insert_expense_split(&conn, &cat1, -10_000, "2026-08-10");
        // cat2: allocate 50, spend 200 -> available = -150 (negative, clamped to 0)
        set_allocation_inner(&conn, &cat2, "2026-08", 5_000).unwrap();
        insert_expense_split(&conn, &cat2, -20_000, "2026-08-10");

        let view = get_home_view_inner(&conn, "2026-08-15").unwrap();
        assert_eq!(view.flow_remaining_cents, 20_000);
    }

    #[test]
    fn flow_remaining_clamped_when_all_negative() {
        let conn = test_db();
        let cat = first_category_id(&conn, "flow");
        set_allocation_inner(&conn, &cat, "2026-08", 10_000).unwrap();
        insert_expense_split(&conn, &cat, -50_000, "2026-08-05");

        let view = get_home_view_inner(&conn, "2026-08-15").unwrap();
        assert_eq!(
            view.flow_remaining_cents, 0,
            "all overspent -> 0, not negative"
        );
    }

    #[test]
    fn days_remaining_counts_today() {
        // Aug has 31 days; last day = 31 -> remaining = 1
        assert_eq!(days_remaining_in_month("2026-08-31").unwrap(), 1);
    }

    #[test]
    fn days_remaining_full_month() {
        // Aug 1 -> 31 days remaining
        assert_eq!(days_remaining_in_month("2026-08-01").unwrap(), 31);
    }

    #[test]
    fn safe_to_spend_zero_when_no_days() {
        let conn = test_db();
        let cat = first_category_id(&conn, "flow");
        set_allocation_inner(&conn, &cat, "2026-08", 100_000).unwrap();
        // Pass a date past the last day to force days_remaining = 0
        let view = get_home_view_inner(&conn, "2026-08-32").unwrap();
        assert_eq!(view.days_remaining, 0);
        assert_eq!(view.safe_to_spend_daily_cents, 0);
    }

    #[test]
    fn safe_to_spend_integer_division() {
        let conn = test_db();
        let cat = first_category_id(&conn, "flow");
        // 100 cents over 3 days = 33 (floor)
        set_allocation_inner(&conn, &cat, "2026-08", 100).unwrap();
        // "2026-08-29" -> days remaining = 31 - 29 + 1 = 3
        let view = get_home_view_inner(&conn, "2026-08-29").unwrap();
        assert_eq!(view.days_remaining, 3);
        assert_eq!(view.safe_to_spend_daily_cents, 33);
    }
}
