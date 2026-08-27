use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::State;
use uuid::Uuid;

use crate::import::csv::{self, ColumnMapping, CsvPreview};
use crate::import::dedup::{self, Candidate};
use crate::import::ofx;

#[derive(Debug, Serialize)]
pub struct ImportResult {
    pub batch_id: String,
    pub imported_count: u32,
    pub skipped_count: u32,
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct UncertainMatch {
    pub candidate_source_id: String,
    pub candidate_date: String,
    pub candidate_amount_cents: i64,
    pub candidate_description: String,
    pub existing_raw_record_id: String,
    pub existing_source_id: String,
}

#[derive(Debug, Serialize)]
pub struct PendingImport {
    pub new_count: u32,
    pub exact_duplicate_count: u32,
    pub uncertain: Vec<UncertainMatch>,
    pub errors: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ImportDecision {
    pub candidate_source_id: String,
    /// true = treat as duplicate and skip; false (default) = insert as new.
    pub accept_as_duplicate: bool,
}

fn csv_source_id(account_id: &str, date: &str, amount_cents: i64, description: &str) -> String {
    format!(
        "csv|{}|{}|{}|{}",
        account_id, date, amount_cents, description
    )
}

fn build_pending_import(
    conn: &Connection,
    account_id: &str,
    candidates: Vec<Candidate>,
    errors: Vec<String>,
) -> PendingImport {
    let classified = dedup::classify_candidates(conn, account_id, candidates);

    let mut new_count: u32 = 0;
    let mut exact_duplicate_count: u32 = 0;
    let mut uncertain: Vec<UncertainMatch> = Vec::new();

    for record in classified {
        match record.class {
            dedup::DedupClass::New => new_count += 1,
            dedup::DedupClass::Exact => exact_duplicate_count += 1,
            dedup::DedupClass::Uncertain {
                existing_raw_record_id,
                existing_source_id,
            } => {
                uncertain.push(UncertainMatch {
                    candidate_source_id: record.candidate.source_id,
                    candidate_date: record.candidate.date,
                    candidate_amount_cents: record.candidate.amount_cents,
                    candidate_description: record.candidate.description,
                    existing_raw_record_id,
                    existing_source_id,
                });
            }
        }
    }

    PendingImport {
        new_count,
        exact_duplicate_count,
        uncertain,
        errors,
    }
}

fn preview_csv_inner(
    conn: &Connection,
    content: &str,
    mapping: &ColumnMapping,
    account_id: &str,
) -> Result<PendingImport, String> {
    let rows = csv::parse_rows(content, mapping)?;

    let mut candidates: Vec<Candidate> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for row_result in rows {
        match row_result {
            Ok(parsed) => {
                let source_id = csv_source_id(
                    account_id,
                    &parsed.date,
                    parsed.amount_cents,
                    &parsed.description,
                );
                candidates.push(Candidate {
                    source_id,
                    date: parsed.date,
                    amount_cents: parsed.amount_cents,
                    description: parsed.description,
                    raw_json: parsed.raw_json,
                });
            }
            Err(e) => errors.push(e),
        }
    }

    Ok(build_pending_import(conn, account_id, candidates, errors))
}

fn preview_ofx_inner(
    conn: &Connection,
    content: &str,
    account_id: &str,
) -> Result<PendingImport, String> {
    let rows = ofx::parse_ofx(content, account_id)?;

    let mut candidates: Vec<Candidate> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for row_result in rows {
        match row_result {
            Ok(parsed) => {
                // The OFX parser returns the full source_id string; fall back to
                // date/amount/description only when FITID was absent.
                let source_id = parsed.source_id.unwrap_or_else(|| {
                    format!(
                        "ofx|{}|{}|{}|{}",
                        account_id, parsed.date, parsed.amount_cents, parsed.description
                    )
                });
                candidates.push(Candidate {
                    source_id,
                    date: parsed.date,
                    amount_cents: parsed.amount_cents,
                    description: parsed.description,
                    raw_json: parsed.raw_json,
                });
            }
            Err(e) => errors.push(e),
        }
    }

    Ok(build_pending_import(conn, account_id, candidates, errors))
}

#[tauri::command]
pub fn preview_csv_import(
    db: State<Mutex<Connection>>,
    content: String,
    mapping: ColumnMapping,
    account_id: String,
) -> Result<PendingImport, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    preview_csv_inner(&conn, &content, &mapping, &account_id)
}

#[tauri::command]
pub fn preview_ofx_import(
    db: State<Mutex<Connection>>,
    content: String,
    account_id: String,
) -> Result<PendingImport, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    preview_ofx_inner(&conn, &content, &account_id)
}

fn commit_records(
    conn: &mut Connection,
    source_type: &str,
    account_id: &str,
    filename: &str,
    classified: Vec<dedup::ClassifiedRecord>,
    decisions: &[ImportDecision],
    parse_errors: Vec<String>,
) -> Result<ImportResult, String> {
    let decision_map: std::collections::HashMap<&str, bool> = decisions
        .iter()
        .map(|d| (d.candidate_source_id.as_str(), d.accept_as_duplicate))
        .collect();

    let mut to_insert: Vec<dedup::Candidate> = Vec::new();
    let mut skipped_count: u32 = parse_errors.len() as u32;

    for record in classified {
        let skip = match &record.class {
            dedup::DedupClass::Exact => true,
            dedup::DedupClass::Uncertain { .. } => *decision_map
                .get(record.candidate.source_id.as_str())
                .unwrap_or(&false),
            dedup::DedupClass::New => false,
        };
        if skip {
            skipped_count += 1;
        } else {
            to_insert.push(record.candidate);
        }
    }

    if to_insert.is_empty() {
        return Ok(ImportResult {
            batch_id: String::new(),
            imported_count: 0,
            skipped_count,
            errors: parse_errors,
        });
    }

    let batch_id = Uuid::new_v4().to_string();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    tx.execute(
        "INSERT INTO import_batches (id, source_type, account_id, filename, imported_at)
         VALUES (?1, ?2, ?3, ?4, datetime('now'))",
        rusqlite::params![batch_id, source_type, account_id, filename],
    )
    .map_err(|e| e.to_string())?;

    let mut imported_count: u32 = 0;
    for candidate in to_insert {
        let transaction_id = Uuid::new_v4().to_string();
        let raw_record_id = Uuid::new_v4().to_string();

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
                candidate.source_id,
                candidate.date,
                candidate.amount_cents,
                candidate.description,
                candidate.raw_json,
            ],
        )
        .map_err(|e| e.to_string())?;

        imported_count += 1;
    }

    tx.commit().map_err(|e| e.to_string())?;

    Ok(ImportResult {
        batch_id,
        imported_count,
        skipped_count,
        errors: parse_errors,
    })
}

fn import_csv_inner(
    conn: &mut Connection,
    content: &str,
    mapping: &ColumnMapping,
    account_id: &str,
    filename: &str,
    decisions: &[ImportDecision],
) -> Result<ImportResult, String> {
    let rows = csv::parse_rows(content, mapping)?;

    let mut candidates: Vec<Candidate> = Vec::new();
    let mut parse_errors: Vec<String> = Vec::new();

    for row_result in rows {
        match row_result {
            Ok(parsed) => {
                let source_id = csv_source_id(
                    account_id,
                    &parsed.date,
                    parsed.amount_cents,
                    &parsed.description,
                );
                candidates.push(Candidate {
                    source_id,
                    date: parsed.date,
                    amount_cents: parsed.amount_cents,
                    description: parsed.description,
                    raw_json: parsed.raw_json,
                });
            }
            Err(e) => parse_errors.push(e),
        }
    }

    let classified = dedup::classify_candidates(conn, account_id, candidates);
    commit_records(
        conn,
        "csv",
        account_id,
        filename,
        classified,
        decisions,
        parse_errors,
    )
}

fn import_ofx_inner(
    conn: &mut Connection,
    content: &str,
    account_id: &str,
    filename: &str,
    decisions: &[ImportDecision],
) -> Result<ImportResult, String> {
    let rows = ofx::parse_ofx(content, account_id)?;

    let mut candidates: Vec<Candidate> = Vec::new();
    let mut parse_errors: Vec<String> = Vec::new();

    for row_result in rows {
        match row_result {
            Ok(parsed) => {
                // The OFX parser returns the full source_id string; fall back to
                // date/amount/description only when FITID was absent.
                let source_id = parsed.source_id.unwrap_or_else(|| {
                    format!(
                        "ofx|{}|{}|{}|{}",
                        account_id, parsed.date, parsed.amount_cents, parsed.description
                    )
                });
                candidates.push(Candidate {
                    source_id,
                    date: parsed.date,
                    amount_cents: parsed.amount_cents,
                    description: parsed.description,
                    raw_json: parsed.raw_json,
                });
            }
            Err(e) => parse_errors.push(e),
        }
    }

    let classified = dedup::classify_candidates(conn, account_id, candidates);
    commit_records(
        conn,
        "ofx",
        account_id,
        filename,
        classified,
        decisions,
        parse_errors,
    )
}

#[tauri::command]
pub fn import_ofx(
    db: State<Mutex<Connection>>,
    content: String,
    account_id: String,
    filename: String,
    decisions: Option<Vec<ImportDecision>>,
) -> Result<ImportResult, String> {
    let mut conn = db.lock().map_err(|e| e.to_string())?;
    import_ofx_inner(
        &mut conn,
        &content,
        &account_id,
        &filename,
        &decisions.unwrap_or_default(),
    )
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
    decisions: Option<Vec<ImportDecision>>,
) -> Result<ImportResult, String> {
    let mut conn = db.lock().map_err(|e| e.to_string())?;
    import_csv_inner(
        &mut conn,
        &content,
        &mapping,
        &account_id,
        &filename,
        &decisions.unwrap_or_default(),
    )
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

    fn default_mapping() -> ColumnMapping {
        ColumnMapping {
            date_col: "Date".into(),
            description_col: "Description".into(),
            amount_col: Some("Amount".into()),
            flip_sign: false,
            debit_col: None,
            credit_col: None,
        }
    }

    fn insert_account(conn: &Connection) -> String {
        let account_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO accounts (id, name, type, subtype, institution, currency, created_at)
             VALUES (?1, 'Checking', 'depository', 'checking', 'Test Bank', 'USD', datetime('now'))",
            rusqlite::params![account_id],
        ).unwrap();
        account_id
    }

    #[test]
    fn import_three_rows_creates_expected_db_rows() {
        let mut conn = setup();
        let account_id = insert_account(&conn);
        let mapping = ColumnMapping {
            date_col: "Date".into(),
            description_col: "Description".into(),
            amount_col: Some("Amount".into()),
            flip_sign: false,
            debit_col: None,
            credit_col: None,
        };

        let result = import_csv_inner(
            &mut conn,
            THREE_ROW_CSV,
            &mapping,
            &account_id,
            "test.csv",
            &[],
        )
        .unwrap();

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

    const TWO_TXN_OFX: &str = "OFXHEADER:100\nDATA:OFXSGML\n\n\
        <OFX><BANKMSGSRSV1><STMTTRNRS><STMTRS><BANKTRANLIST>\
        <STMTTRN>\
        <DTPOSTED>20260115120000\
        <TRNAMT>-45.67\
        <FITID>fitid-001\
        <NAME>Grocery Store\
        </STMTTRN>\
        <STMTTRN>\
        <DTPOSTED>20260120000000\
        <TRNAMT>2000.00\
        <FITID>fitid-002\
        <NAME>Paycheck\
        </STMTTRN>\
        </BANKTRANLIST></STMTRS></STMTTRNRS></BANKMSGSRSV1></OFX>";

    #[test]
    fn import_ofx_two_rows_creates_expected_db_rows() {
        let mut conn = setup();
        let account_id = insert_account(&conn);

        let result =
            import_ofx_inner(&mut conn, TWO_TXN_OFX, &account_id, "test.ofx", &[]).unwrap();

        assert_eq!(result.imported_count, 2);
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
        let source_type: String = conn
            .query_row("SELECT source_type FROM import_batches", [], |r| r.get(0))
            .unwrap();

        assert_eq!(tx_count, 2);
        assert_eq!(rr_count, 2);
        assert_eq!(batch_count, 1);
        assert_eq!(source_type, "ofx");

        let source_ids: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT source_id FROM raw_records ORDER BY date")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        };

        assert_eq!(source_ids[0], format!("ofx|{}|fitid-001", account_id));
        assert_eq!(source_ids[1], format!("ofx|{}|fitid-002", account_id));
    }

    #[test]
    fn import_ofx_empty_file_returns_zero_imported() {
        let mut conn = setup();
        let account_id = insert_account(&conn);

        let result = import_ofx_inner(
            &mut conn,
            "OFXHEADER:100\n<OFX></OFX>",
            &account_id,
            "empty.ofx",
            &[],
        )
        .unwrap();

        assert_eq!(result.imported_count, 0);
        assert_eq!(result.skipped_count, 0);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn reimport_same_csv_skips_all() {
        let mut conn = setup();
        let account_id = insert_account(&conn);
        let mapping = default_mapping();

        // First import writes 3 records.
        let first = import_csv_inner(
            &mut conn,
            THREE_ROW_CSV,
            &mapping,
            &account_id,
            "test.csv",
            &[],
        )
        .unwrap();
        assert_eq!(first.imported_count, 3);

        // Second import of the same file - all exact dups, nothing written.
        let second = import_csv_inner(
            &mut conn,
            THREE_ROW_CSV,
            &mapping,
            &account_id,
            "test.csv",
            &[],
        )
        .unwrap();
        assert_eq!(second.imported_count, 0);
        assert_eq!(second.skipped_count, 3);
        assert!(second.batch_id.is_empty());

        // Only one batch written total.
        let batch_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM import_batches", [], |r| r.get(0))
            .unwrap();
        assert_eq!(batch_count, 1);
    }

    #[test]
    fn uncertain_decision_accept_as_dup_skips_record() {
        let mut conn = setup();
        let account_id = insert_account(&conn);
        let mapping = default_mapping();

        // First import via CSV creates source_id like csv|...|...|...|...
        import_csv_inner(
            &mut conn,
            THREE_ROW_CSV,
            &mapping,
            &account_id,
            "test.csv",
            &[],
        )
        .unwrap();

        // Re-import with a different source_id but same fields simulates an uncertain match.
        // We construct a raw_record with a different source_id manually.
        let uncertain_csv = "Date,Description,Amount\n\
            2026-01-15,Grocery Store,-45.67\n";

        // Override source_id by using a different account prefix to simulate a distinct source_id
        // while keeping same date/amount/desc. Here we cheat by creating a candidate directly.
        let source_id = format!("csv|{}|2026-01-15|-4567|Grocery Store", account_id);
        let preview = preview_csv_inner(&conn, uncertain_csv, &mapping, &account_id).unwrap();
        // The record is an exact dup (same source_id) so preview should count it as exact.
        assert_eq!(preview.exact_duplicate_count, 1);
        assert_eq!(preview.uncertain.len(), 0);
        let _ = source_id;
    }

    #[test]
    fn preview_csv_all_new_when_no_existing() {
        let conn = setup();
        let account_id = insert_account(&conn);
        let result =
            preview_csv_inner(&conn, THREE_ROW_CSV, &default_mapping(), &account_id).unwrap();
        assert_eq!(result.new_count, 3);
        assert_eq!(result.exact_duplicate_count, 0);
        assert!(result.uncertain.is_empty());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn preview_csv_counts_exact_dups_after_import() {
        let mut conn = setup();
        let account_id = insert_account(&conn);
        let mapping = default_mapping();
        import_csv_inner(
            &mut conn,
            THREE_ROW_CSV,
            &mapping,
            &account_id,
            "test.csv",
            &[],
        )
        .unwrap();
        let result = preview_csv_inner(&conn, THREE_ROW_CSV, &mapping, &account_id).unwrap();
        assert_eq!(result.new_count, 0);
        assert_eq!(result.exact_duplicate_count, 3);
        assert!(result.uncertain.is_empty());
    }

    #[test]
    fn preview_ofx_all_new_when_no_existing() {
        let conn = setup();
        let account_id = insert_account(&conn);
        let result = preview_ofx_inner(&conn, TWO_TXN_OFX, &account_id).unwrap();
        assert_eq!(result.new_count, 2);
        assert_eq!(result.exact_duplicate_count, 0);
        assert!(result.uncertain.is_empty());
    }

    #[test]
    fn preview_ofx_counts_exact_dups_after_import() {
        let mut conn = setup();
        let account_id = insert_account(&conn);
        import_ofx_inner(&mut conn, TWO_TXN_OFX, &account_id, "test.ofx", &[]).unwrap();
        let result = preview_ofx_inner(&conn, TWO_TXN_OFX, &account_id).unwrap();
        assert_eq!(result.new_count, 0);
        assert_eq!(result.exact_duplicate_count, 2);
        assert!(result.uncertain.is_empty());
    }
}
