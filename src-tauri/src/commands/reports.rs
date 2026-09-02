use chrono::{Datelike, Utc};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::State;
use ts_rs::TS;

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/lib/generated/CategorySpendReport.ts")]
pub struct CategorySpendReport {
    pub category_id: String,
    pub category_name: String,
    /// "flow" | "sinking"
    pub kind: String,
    /// positive cents: abs value of envelope spending splits for the month
    #[ts(type = "number")]
    pub spent_cents: i64,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/lib/generated/MonthlySpendTrend.ts")]
pub struct MonthlySpendTrend {
    /// YYYY-MM
    pub month: String,
    /// positive cents: total envelope spending for the month
    #[ts(type = "number")]
    pub total_spent_cents: i64,
}

fn get_spending_by_category_inner(
    conn: &Connection,
    month: &str,
) -> Result<Vec<CategorySpendReport>, String> {
    let month_prefix = format!("{month}-%");
    let mut stmt = conn
        .prepare(
            "SELECT c.id, c.name, c.kind, SUM(-s.amount_cents) AS spent_cents
             FROM splits s
             JOIN transactions t ON t.id = s.transaction_id
             JOIN raw_records rr ON rr.transaction_id = t.id
               AND NOT EXISTS (
                   SELECT 1 FROM raw_records rr2 WHERE rr2.supersedes_id = rr.id
               )
             JOIN categories c ON c.id = s.target_id
             WHERE s.target_type = 'envelope'
               AND s.amount_cents < 0
               AND rr.date LIKE ?1
             GROUP BY c.id, c.name, c.kind
             ORDER BY spent_cents DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(rusqlite::params![month_prefix], |row| {
            Ok(CategorySpendReport {
                category_id: row.get(0)?,
                category_name: row.get(1)?,
                kind: row.get(2)?,
                spent_cents: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

fn get_monthly_spend_trend_inner(
    conn: &Connection,
    months: u32,
) -> Result<Vec<MonthlySpendTrend>, String> {
    if months == 0 {
        return Ok(vec![]);
    }

    let now = Utc::now();
    let mut year = now.year();
    let mut month_num = now.month(); // 1-12

    let mut all_months: Vec<String> = Vec::with_capacity(months as usize);
    for _ in 0..months {
        all_months.push(format!("{year:04}-{month_num:02}"));
        if month_num == 1 {
            month_num = 12;
            year -= 1;
        } else {
            month_num -= 1;
        }
    }

    let start_month = all_months.last().cloned().unwrap_or_default();
    let end_month = all_months.first().cloned().unwrap_or_default();

    let mut stmt = conn
        .prepare(
            "SELECT strftime('%Y-%m', rr.date) AS m, SUM(-s.amount_cents) AS spent_cents
             FROM splits s
             JOIN transactions t ON t.id = s.transaction_id
             JOIN raw_records rr ON rr.transaction_id = t.id
               AND NOT EXISTS (
                   SELECT 1 FROM raw_records rr2 WHERE rr2.supersedes_id = rr.id
               )
             WHERE s.target_type = 'envelope'
               AND s.amount_cents < 0
               AND strftime('%Y-%m', rr.date) >= ?1
               AND strftime('%Y-%m', rr.date) <= ?2
             GROUP BY m
             ORDER BY m DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(rusqlite::params![start_month, end_month], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|e| e.to_string())?;

    let mut spend_map: HashMap<String, i64> = HashMap::new();
    for row in rows {
        let (m, spent) = row.map_err(|e| e.to_string())?;
        spend_map.insert(m, spent);
    }

    Ok(all_months
        .into_iter()
        .map(|m| {
            let total_spent_cents = spend_map.get(&m).copied().unwrap_or(0);
            MonthlySpendTrend {
                month: m,
                total_spent_cents,
            }
        })
        .collect())
}

#[tauri::command]
pub fn get_spending_by_category(
    vault: State<'_, crate::crypto::VaultState>,
    db: State<Mutex<Connection>>,
    month: String,
) -> Result<Vec<CategorySpendReport>, String> {
    super::require_unlocked(&vault)?;
    let conn = db.lock().map_err(|e| e.to_string())?;
    get_spending_by_category_inner(&conn, &month)
}

#[tauri::command]
pub fn get_monthly_spend_trend(
    vault: State<'_, crate::crypto::VaultState>,
    db: State<Mutex<Connection>>,
    months: u32,
) -> Result<Vec<MonthlySpendTrend>, String> {
    super::require_unlocked(&vault)?;
    let conn = db.lock().map_err(|e| e.to_string())?;
    get_monthly_spend_trend_inner(&conn, months)
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

    fn insert_category(conn: &Connection, name: &str, kind: &str) -> String {
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO categories (id, name, kind, created_at) VALUES (?1, ?2, ?3, datetime('now'))",
            rusqlite::params![id, name, kind],
        )
        .unwrap();
        id
    }

    fn insert_transaction(conn: &Connection, account_id: &str, date: &str, amount: i64) -> String {
        let tx_id = Uuid::new_v4().to_string();
        let rr_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO transactions (id, account_id, created_at) VALUES (?1, ?2, datetime('now'))",
            rusqlite::params![tx_id, account_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO raw_records
             (id, transaction_id, supersedes_id, import_batch_id, source_id,
              date, amount_cents, description, raw_json, created_at)
             VALUES (?1, ?2, NULL, NULL, ?3, ?4, ?5, 'Merchant', '{}', datetime('now'))",
            rusqlite::params![rr_id, tx_id, format!("test|{}", rr_id), date, amount],
        )
        .unwrap();
        tx_id
    }

    fn insert_split(conn: &Connection, transaction_id: &str, category_id: &str, amount: i64) {
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO splits (id, transaction_id, target_type, target_id, amount_cents)
             VALUES (?1, ?2, 'envelope', ?3, ?4)",
            rusqlite::params![id, transaction_id, category_id, amount],
        )
        .unwrap();
    }

    #[test]
    fn category_spend_sums_correctly_for_month() {
        let conn = setup();
        let account_id = insert_account(&conn);
        let cat_id = insert_category(&conn, "Groceries", "flow");

        let tx1 = insert_transaction(&conn, &account_id, "2026-01-10", -2000);
        insert_split(&conn, &tx1, &cat_id, -2000);

        let tx2 = insert_transaction(&conn, &account_id, "2026-01-20", -3000);
        insert_split(&conn, &tx2, &cat_id, -3000);

        let result = get_spending_by_category_inner(&conn, "2026-01").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].category_id, cat_id);
        assert_eq!(result[0].spent_cents, 5000);
        assert_eq!(result[0].kind, "flow");
    }

    #[test]
    fn category_spend_excludes_other_months() {
        let conn = setup();
        let account_id = insert_account(&conn);
        let cat_id = insert_category(&conn, "Groceries", "flow");

        let tx_jan = insert_transaction(&conn, &account_id, "2026-01-15", -1000);
        insert_split(&conn, &tx_jan, &cat_id, -1000);

        let tx_feb = insert_transaction(&conn, &account_id, "2026-02-05", -2000);
        insert_split(&conn, &tx_feb, &cat_id, -2000);

        let jan = get_spending_by_category_inner(&conn, "2026-01").unwrap();
        assert_eq!(jan[0].spent_cents, 1000);

        let feb = get_spending_by_category_inner(&conn, "2026-02").unwrap();
        assert_eq!(feb[0].spent_cents, 2000);
    }

    #[test]
    fn category_spend_excludes_refunds() {
        let conn = setup();
        let account_id = insert_account(&conn);
        let cat_id = insert_category(&conn, "Groceries", "flow");

        let tx_spend = insert_transaction(&conn, &account_id, "2026-01-10", -5000);
        insert_split(&conn, &tx_spend, &cat_id, -5000);

        let tx_refund = insert_transaction(&conn, &account_id, "2026-01-15", 1000);
        insert_split(&conn, &tx_refund, &cat_id, 1000);

        let result = get_spending_by_category_inner(&conn, "2026-01").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].spent_cents, 5000); // refund excluded
    }

    #[test]
    fn monthly_trend_returns_correct_count() {
        let conn = setup();
        let result = get_monthly_spend_trend_inner(&conn, 3).unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn monthly_trend_zero_months_returns_empty() {
        let conn = setup();
        let result = get_monthly_spend_trend_inner(&conn, 0).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn monthly_trend_empty_months_have_zero_spend() {
        let conn = setup();
        let result = get_monthly_spend_trend_inner(&conn, 6).unwrap();
        assert_eq!(result.len(), 6);
        assert!(result.iter().all(|r| r.total_spent_cents == 0));
    }

    #[test]
    fn monthly_trend_ordered_newest_first() {
        let conn = setup();
        let result = get_monthly_spend_trend_inner(&conn, 6).unwrap();
        for i in 0..result.len() - 1 {
            assert!(
                result[i].month > result[i + 1].month,
                "months must be newest first"
            );
        }
    }

    #[test]
    fn monthly_trend_includes_spend_in_range() {
        let conn = setup();
        let account_id = insert_account(&conn);
        let cat_id = insert_category(&conn, "Gas", "flow");

        let now = Utc::now();
        let this_month = format!("{:04}-{:02}-15", now.year(), now.month());
        let tx = insert_transaction(&conn, &account_id, &this_month, -4000);
        insert_split(&conn, &tx, &cat_id, -4000);

        let result = get_monthly_spend_trend_inner(&conn, 3).unwrap();
        let current = &result[0]; // newest first
        assert_eq!(
            current.month,
            format!("{:04}-{:02}", now.year(), now.month())
        );
        assert_eq!(current.total_spent_cents, 4000);
    }
}
