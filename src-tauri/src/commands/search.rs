use rusqlite::Connection;
use std::sync::Mutex;
use tauri::State;

use crate::commands::transactions::TransactionRow;

/// Rebuilds the FTS index from the live raw_records + transaction_meta join.
/// Call after any mutation that inserts new raw_records or changes notes.
pub(crate) fn rebuild_fts(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "DELETE FROM transactions_fts;
         INSERT INTO transactions_fts (rowid, description, notes, transaction_id)
         SELECT rr.rowid, rr.description, COALESCE(tm.notes, ''), rr.transaction_id
         FROM raw_records rr
         LEFT JOIN transaction_meta tm ON tm.transaction_id = rr.transaction_id
         WHERE NOT EXISTS (
             SELECT 1 FROM raw_records rr2 WHERE rr2.supersedes_id = rr.id
         );",
    )
}

fn sanitize_fts_query(raw: &str) -> String {
    format!("\"{}\"", raw.replace('"', "\"\""))
}

fn search_transactions_inner(
    conn: &Connection,
    query: &str,
) -> Result<Vec<TransactionRow>, String> {
    if query.is_empty() {
        return Ok(vec![]);
    }
    let sanitized = sanitize_fts_query(query);
    let mut stmt = conn
        .prepare(
            "SELECT t.id, t.account_id, a.name,
                    rr.date, rr.amount_cents, rr.description,
                    COALESCE(tm.notes, '') AS notes,
                    COALESCE(tm.tags, '[]') AS tags,
                    COALESCE(tm.reviewed, 0) AS reviewed,
                    s.target_type AS category_type,
                    s.target_id AS category_id,
                    COALESCE(cat.name, ic.name) AS category_name
             FROM transactions_fts fts
             JOIN raw_records rr ON rr.rowid = fts.rowid
             JOIN transactions t ON t.id = rr.transaction_id
             JOIN accounts a ON t.account_id = a.id
             LEFT JOIN transaction_meta tm ON tm.transaction_id = t.id
             LEFT JOIN splits s ON s.transaction_id = t.id
             LEFT JOIN categories cat ON cat.id = s.target_id AND s.target_type = 'envelope'
             LEFT JOIN income_categories ic ON ic.id = s.target_id AND s.target_type = 'income'
             WHERE transactions_fts MATCH ?1
             ORDER BY rank",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(rusqlite::params![sanitized], |row| {
            let tags_json: String = row.get(7)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                tags_json,
                row.get::<_, i64>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, Option<String>>(11)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for row in rows {
        let (
            id,
            account_id,
            account_name,
            date,
            amount_cents,
            description,
            notes,
            tags_json,
            reviewed_int,
            category_type,
            category_id,
            category_name,
        ) = row.map_err(|e| e.to_string())?;
        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
        result.push(TransactionRow {
            id,
            account_id,
            account_name,
            date,
            amount_cents,
            description,
            notes,
            tags,
            reviewed: reviewed_int != 0,
            category_type,
            category_id,
            category_name,
        });
    }
    Ok(result)
}

#[tauri::command]
pub fn search_transactions(
    vault: State<'_, crate::crypto::VaultState>,
    db: State<Mutex<Connection>>,
    query: String,
) -> Result<Vec<TransactionRow>, String> {
    super::require_unlocked(&vault)?;
    if query.is_empty() {
        return Ok(vec![]);
    }
    let conn = db.lock().map_err(|e| e.to_string())?;
    search_transactions_inner(&conn, &query)
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

    fn insert_transaction(conn: &Connection, account_id: &str, description: &str) -> String {
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
             VALUES (?1, ?2, NULL, NULL, ?3, '2026-01-15', -1000, ?4, '{}', datetime('now'))",
            rusqlite::params![rr_id, tx_id, format!("test|{}", rr_id), description],
        )
        .unwrap();
        tx_id
    }

    #[test]
    fn empty_query_returns_empty() {
        let conn = setup();
        let result = search_transactions_inner(&conn, "").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn no_match_returns_empty() {
        let conn = setup();
        let account_id = insert_account(&conn);
        insert_transaction(&conn, &account_id, "Coffee Shop");
        rebuild_fts(&conn).unwrap();

        let result = search_transactions_inner(&conn, "xyznosuchthing").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn search_finds_by_description() {
        let conn = setup();
        let account_id = insert_account(&conn);
        let target_id = insert_transaction(&conn, &account_id, "Whole Foods Market");
        insert_transaction(&conn, &account_id, "Coffee Shop");
        rebuild_fts(&conn).unwrap();

        let result = search_transactions_inner(&conn, "Whole Foods").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, target_id);
    }

    #[test]
    fn search_finds_by_notes() {
        let conn = setup();
        let account_id = insert_account(&conn);
        let target_id = insert_transaction(&conn, &account_id, "General Store");
        insert_transaction(&conn, &account_id, "Coffee Shop");

        conn.execute(
            "INSERT INTO transaction_meta (transaction_id, notes, tags, reviewed, updated_at)
             VALUES (?1, 'birthday gift purchase', '[]', 0, datetime('now'))",
            rusqlite::params![target_id],
        )
        .unwrap();

        rebuild_fts(&conn).unwrap();

        let result = search_transactions_inner(&conn, "birthday").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, target_id);
    }

    #[test]
    fn search_transactions_gate_rejects_locked() {
        use crate::crypto::VaultState;
        use std::sync::Mutex;
        let vault = VaultState(Mutex::new(None));
        assert_eq!(
            crate::commands::require_unlocked(&vault).unwrap_err(),
            "locked"
        );
    }
}
