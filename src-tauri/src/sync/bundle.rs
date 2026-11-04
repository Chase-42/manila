use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::crypto::field::open_bytes;

pub const SYNCED_TABLES: &[(&str, &str)] = &[
    ("accounts", "id"),
    ("transactions", "id"),
    ("raw_records", "id"),
    ("import_batches", "id"),
    ("postings", "id"),
    ("transaction_meta", "transaction_id"),
    ("splits", "id"),
    ("categories", "id"),
    ("category_groups", "id"),
    ("income_categories", "id"),
    ("allocation_events", "id"),
    ("month_closes", "month"),
    ("goals", "id"),
    ("valuation_snapshots", "id"),
    ("categorization_rules", "id"),
    ("plaid_connections", "id"),
];

#[derive(Serialize, Deserialize)]
pub struct SyncBundle {
    pub schema_version: u32,
    pub device_id: String,
    pub push_counter: i64,
    pub vector_clock: HashMap<String, i64>,
    pub changes: Vec<BundleChange>,
}

#[derive(Serialize, Deserialize)]
pub struct BundleChange {
    pub table: String,
    pub op: String,
    pub row_json: Option<serde_json::Value>,
    pub row_id: Option<String>,
}

pub fn build_push_bundle(
    conn: &Connection,
    device_id: &str,
    push_counter: i64,
) -> Result<Option<Vec<u8>>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, table_name, row_id, operation
             FROM sync_pending_changes
             ORDER BY id ASC",
        )
        .map_err(|e| e.to_string())?;

    struct PendingRow {
        id: i64,
        table_name: String,
        row_id: String,
        operation: String,
    }

    let rows: Vec<PendingRow> = stmt
        .query_map([], |r| {
            Ok(PendingRow {
                id: r.get(0)?,
                table_name: r.get(1)?,
                row_id: r.get(2)?,
                operation: r.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    if rows.is_empty() {
        return Ok(None);
    }

    // Dedup: keep the highest-id record per (table_name, row_id).
    let mut dedup: std::collections::BTreeMap<(String, String), (i64, String)> =
        std::collections::BTreeMap::new();
    for row in &rows {
        let key = (row.table_name.clone(), row.row_id.clone());
        let entry = dedup.entry(key).or_insert((row.id, row.operation.clone()));
        if row.id > entry.0 {
            *entry = (row.id, row.operation.clone());
        }
    }

    let mut changes: Vec<BundleChange> = Vec::new();

    for ((table_name, row_id), (_id, op)) in &dedup {
        let pk_col = match SYNCED_TABLES.iter().find(|(t, _)| *t == table_name) {
            Some((_, pk)) => *pk,
            None => continue,
        };

        if op == "delete" {
            changes.push(BundleChange {
                table: table_name.clone(),
                op: "delete".to_string(),
                row_json: None,
                row_id: Some(row_id.clone()),
            });
        } else {
            let query = format!("SELECT * FROM \"{table_name}\" WHERE \"{pk_col}\" = ?1");
            let row_json: Option<serde_json::Value> = {
                let mut row_stmt =
                    conn.prepare(&query).map_err(|e| e.to_string())?;
                let mut row_iter = row_stmt
                    .query([row_id.as_str()])
                    .map_err(|e| e.to_string())?;
                match row_iter.next().map_err(|e| e.to_string())? {
                    None => None,
                    Some(row) => {
                        let col_count = row.as_ref().column_count();
                        let col_names: Vec<String> = row
                            .as_ref()
                            .column_names()
                            .into_iter()
                            .map(|s| s.to_string())
                            .collect();
                        let mut obj = serde_json::Map::new();
                        for i in 0..col_count {
                            let val = match row.get_ref(i).map_err(|e| e.to_string())? {
                                rusqlite::types::ValueRef::Null => serde_json::Value::Null,
                                rusqlite::types::ValueRef::Integer(n) => {
                                    serde_json::Value::Number(n.into())
                                }
                                rusqlite::types::ValueRef::Real(f) => {
                                    serde_json::json!(f)
                                }
                                rusqlite::types::ValueRef::Text(t) => serde_json::Value::String(
                                    String::from_utf8_lossy(t).into_owned(),
                                ),
                                rusqlite::types::ValueRef::Blob(b) => {
                                    use base64::{engine::general_purpose::STANDARD as B64, Engine};
                                    serde_json::Value::String(B64.encode(b))
                                }
                            };
                            obj.insert(col_names[i].clone(), val);
                        }
                        Some(serde_json::Value::Object(obj))
                    }
                }
            };

            match row_json {
                None => {
                    // Row is gone — if no delete record exists, omit the upsert from bundle.
                }
                Some(json) => {
                    changes.push(BundleChange {
                        table: table_name.clone(),
                        op: "upsert".to_string(),
                        row_json: Some(json),
                        row_id: None,
                    });
                }
            }
        }
    }

    if changes.is_empty() {
        return Ok(None);
    }

    // Read vector clock.
    let mut clock: HashMap<String, i64> = HashMap::new();
    let mut vc_stmt = conn
        .prepare("SELECT table_name, local_seq FROM sync_vector_clock")
        .map_err(|e| e.to_string())?;
    let vc_rows: Vec<(String, i64)> = vc_stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for (table, seq) in vc_rows {
        clock.insert(table, seq);
    }

    let bundle = SyncBundle {
        schema_version: 1,
        device_id: device_id.to_string(),
        push_counter,
        vector_clock: clock,
        changes,
    };

    let json = serde_json::to_vec(&bundle).map_err(|e| e.to_string())?;
    Ok(Some(json))
}

pub fn decode_bundle(data_key: &[u8; 32], payload_b64: &str) -> Result<SyncBundle, String> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    let ciphertext = B64.decode(payload_b64).map_err(|e| e.to_string())?;
    let plaintext = open_bytes(data_key, &ciphertext).map_err(|e| e.to_string())?;
    serde_json::from_slice(&plaintext).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::open_connection;
    use crate::storage::migrations::run_migrations;

    fn setup() -> Connection {
        let mut conn = open_connection(":memory:", None).unwrap();
        run_migrations(&mut conn).unwrap();
        conn
    }

    fn insert_account(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO accounts (id, name, type, subtype, institution, currency, created_at)
             VALUES (?1, 'Test', 'depository', 'checking', 'Bank', 'USD', datetime('now'))",
            [id],
        )
        .unwrap();
    }

    #[test]
    fn no_pending_returns_none() {
        let conn = setup();
        let result = build_push_bundle(&conn, "dev-1", 0).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn insert_produces_one_upsert_change() {
        let conn = setup();
        insert_account(&conn, "acc-1");

        let bytes = build_push_bundle(&conn, "dev-1", 0).unwrap().unwrap();
        let bundle: SyncBundle = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(bundle.changes.len(), 1);
        assert_eq!(bundle.changes[0].op, "upsert");
        assert_eq!(bundle.changes[0].table, "accounts");
        let row = bundle.changes[0].row_json.as_ref().unwrap();
        assert_eq!(row["id"], serde_json::json!("acc-1"));
    }

    #[test]
    fn two_updates_to_same_row_deduped_to_one_upsert() {
        let conn = setup();
        insert_account(&conn, "acc-1");
        conn.execute("UPDATE accounts SET name = 'Updated' WHERE id = 'acc-1'", [])
            .unwrap();

        let bytes = build_push_bundle(&conn, "dev-1", 0).unwrap().unwrap();
        let bundle: SyncBundle = serde_json::from_slice(&bytes).unwrap();
        // accounts trigger fires, plus seeded categories etc. — filter to accounts only
        let acc_changes: Vec<_> = bundle
            .changes
            .iter()
            .filter(|c| c.table == "accounts")
            .collect();
        assert_eq!(acc_changes.len(), 1);
        assert_eq!(acc_changes[0].op, "upsert");
    }

    #[test]
    fn upsert_then_delete_produces_one_delete() {
        let conn = setup();
        insert_account(&conn, "acc-1");
        conn.execute("DELETE FROM accounts WHERE id = 'acc-1'", [])
            .unwrap();

        let bytes = build_push_bundle(&conn, "dev-1", 0).unwrap().unwrap();
        let bundle: SyncBundle = serde_json::from_slice(&bytes).unwrap();
        let acc_changes: Vec<_> = bundle
            .changes
            .iter()
            .filter(|c| c.table == "accounts")
            .collect();
        assert_eq!(acc_changes.len(), 1);
        assert_eq!(acc_changes[0].op, "delete");
        assert_eq!(acc_changes[0].row_id.as_deref(), Some("acc-1"));
    }

    #[test]
    fn transaction_meta_row_id_uses_transaction_id() {
        let conn = setup();
        insert_account(&conn, "acc-1");
        conn.execute(
            "INSERT INTO transactions (id, account_id, created_at) VALUES ('txn-1', 'acc-1', datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transaction_meta (transaction_id, notes, tags, reviewed, updated_at)
             VALUES ('txn-1', 'note', '[]', 0, datetime('now'))",
            [],
        )
        .unwrap();

        let bytes = build_push_bundle(&conn, "dev-1", 0).unwrap().unwrap();
        let bundle: SyncBundle = serde_json::from_slice(&bytes).unwrap();
        let tm: Vec<_> = bundle
            .changes
            .iter()
            .filter(|c| c.table == "transaction_meta")
            .collect();
        assert_eq!(tm.len(), 1);
        let row = tm[0].row_json.as_ref().unwrap();
        assert_eq!(row["transaction_id"], serde_json::json!("txn-1"));
    }

    #[test]
    fn gone_row_upsert_omitted_when_delete_record_exists() {
        let conn = setup();
        insert_account(&conn, "acc-1");
        conn.execute("DELETE FROM accounts WHERE id = 'acc-1'", [])
            .unwrap();

        // After upsert+delete, the dedup produces a 'delete' op, not an upsert.
        let bytes = build_push_bundle(&conn, "dev-1", 0).unwrap().unwrap();
        let bundle: SyncBundle = serde_json::from_slice(&bytes).unwrap();
        let acc_upserts: Vec<_> = bundle
            .changes
            .iter()
            .filter(|c| c.table == "accounts" && c.op == "upsert")
            .collect();
        assert!(acc_upserts.is_empty(), "upsert should be omitted when delete follows");
    }
}
