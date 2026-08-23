use std::sync::Mutex;
use tauri::State;
use rusqlite::Connection;
use serde::Serialize;
use uuid::Uuid;

use crate::import::csv::{self, ColumnMapping, CsvPreview};

#[derive(Debug, Serialize)]
pub struct ImportResult {
    pub batch_id: String,
    pub imported_count: u32,
    pub skipped_count: u32,
    pub errors: Vec<String>,
}

fn import_csv_inner(
    conn: &mut Connection,
    content: &str,
    mapping: &ColumnMapping,
    account_id: &str,
    filename: &str,
) -> Result<ImportResult, String> {
    let rows = csv::parse_rows(content, mapping)?;

    let batch_id = Uuid::new_v4().to_string();
    let mut imported_count: u32 = 0;
    let mut skipped_count: u32 = 0;
    let mut errors: Vec<String> = Vec::new();

    let tx = conn.transaction().map_err(|e| e.to_string())?;

    tx.execute(
        "INSERT INTO import_batches (id, source_type, account_id, filename, imported_at)
         VALUES (?1, 'csv', ?2, ?3, datetime('now'))",
        rusqlite::params![batch_id, account_id, filename],
    )
    .map_err(|e| e.to_string())?;

    for row_result in rows {
        match row_result {
            Ok(parsed) => {
                let transaction_id = Uuid::new_v4().to_string();
                let raw_record_id = Uuid::new_v4().to_string();
                let source_id = format!(
                    "csv|{}|{}|{}|{}",
                    account_id, parsed.date, parsed.amount_cents, parsed.description
                );

                tx.execute(
                    "INSERT INTO transactions (id, account_id, created_at)
                     VALUES (?1, ?2, datetime('now'))",
                    rusqlite::params![transaction_id, account_id],
                )
                .map_err(|e| e.to_string())?;

                tx.execute(
                    "INSERT INTO raw_records
                     (id, transaction_id, supersedes_id, import_batch_id,
                      source_id, date, amount_cents, description, raw_json, created_at)
                     VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7, ?8, datetime('now'))",
                    rusqlite::params![
                        raw_record_id,
                        transaction_id,
                        batch_id,
                        source_id,
                        parsed.date,
                        parsed.amount_cents,
                        parsed.description,
                        parsed.raw_json,
                    ],
                )
                .map_err(|e| e.to_string())?;

                imported_count += 1;
            }
            Err(e) => {
                errors.push(e);
                skipped_count += 1;
            }
        }
    }

    tx.commit().map_err(|e| e.to_string())?;

    Ok(ImportResult { batch_id, imported_count, skipped_count, errors })
}

#[tauri::command]
pub fn parse_csv_preview(content: String) -> Result<CsvPreview, String> {
    csv::extract_preview(&content)
}

#[tauri::command]
pub fn import_csv(
    db: State<Mutex<Connection>>,
    content: String,
    mapping: ColumnMapping,
    account_id: String,
    filename: String,
) -> Result<ImportResult, String> {
    let mut conn = db.lock().map_err(|e| e.to_string())?;
    import_csv_inner(&mut conn, &content, &mapping, &account_id, &filename)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::open_connection;
    use crate::storage::migrations::run_migrations;

    fn setup() -> Connection {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();
        conn
    }

    const THREE_ROW_CSV: &str = "Date,Description,Amount\n\
        2026-01-15,Grocery Store,-45.67\n\
        2026-01-16,Gas Station,-32.10\n\
        2026-01-17,Paycheck,2000.00\n";

    #[test]
    fn import_three_rows_creates_expected_db_rows() {
        let mut conn = setup();

        let account_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO accounts
             (id, name, type, subtype, institution, currency, created_at)
             VALUES (?1, 'Checking', 'depository', 'checking', 'Test Bank', 'USD', datetime('now'))",
            rusqlite::params![account_id],
        )
        .unwrap();

        let mapping = ColumnMapping {
            date_col: "Date".into(),
            description_col: "Description".into(),
            amount_col: Some("Amount".into()),
            flip_sign: false,
            debit_col: None,
            credit_col: None,
        };

        let result =
            import_csv_inner(&mut conn, THREE_ROW_CSV, &mapping, &account_id, "test.csv").unwrap();

        assert_eq!(result.imported_count, 3);
        assert_eq!(result.skipped_count, 0);
        assert!(result.errors.is_empty());

        let tx_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM transactions", [], |r| r.get(0))
            .unwrap();
        let rr_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM raw_records", [], |r| r.get(0))
            .unwrap();
        let batch_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM import_batches", [], |r| r.get(0))
            .unwrap();

        assert_eq!(tx_count, 3);
        assert_eq!(rr_count, 3);
        assert_eq!(batch_count, 1);

        let source_ids: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT source_id FROM raw_records ORDER BY date")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        };

        assert_eq!(
            source_ids[0],
            format!("csv|{}|2026-01-15|-4567|Grocery Store", account_id)
        );
        assert_eq!(
            source_ids[1],
            format!("csv|{}|2026-01-16|-3210|Gas Station", account_id)
        );
        assert_eq!(
            source_ids[2],
            format!("csv|{}|2026-01-17|200000|Paycheck", account_id)
        );
    }
}
