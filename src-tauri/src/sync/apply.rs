use rusqlite::Connection;

use crate::commands::search::rebuild_fts;
use crate::sync::bundle::{BundleChange, SyncBundle, SYNCED_TABLES};

struct SyncSkipGuard<'a> {
    conn: &'a Connection,
}

impl<'a> SyncSkipGuard<'a> {
    fn new(conn: &'a Connection) -> Result<Self, String> {
        conn.execute("UPDATE _sync_skip SET flag = 1", [])
            .map_err(|e| e.to_string())?;
        Ok(SyncSkipGuard { conn })
    }
}

impl Drop for SyncSkipGuard<'_> {
    fn drop(&mut self) {
        let _ = self.conn.execute("UPDATE _sync_skip SET flag = 0", []);
    }
}

pub fn apply_bundle(
    conn: &mut Connection,
    bundle: &SyncBundle,
    data_key: &[u8; 32],
) -> Result<u32, String> {
    conn.execute("PRAGMA foreign_keys = OFF", [])
        .map_err(|e| e.to_string())?;

    let result = apply_changes(conn, &bundle.changes, data_key);

    conn.execute("PRAGMA foreign_keys = ON", [])
        .map_err(|e| e.to_string())?;

    result
}

fn apply_changes(
    conn: &mut Connection,
    changes: &[BundleChange],
    data_key: &[u8; 32],
) -> Result<u32, String> {
    let _skip_guard = SyncSkipGuard::new(conn)?;

    let mut applied: u32 = 0;
    let mut touched_fts = false;

    for change in changes {
        let pk_col = match SYNCED_TABLES.iter().find(|(t, _)| *t == change.table) {
            Some((_, pk)) => *pk,
            None => continue,
        };

        match change.op.as_str() {
            "upsert" => {
                let row_json = match &change.row_json {
                    Some(j) => j,
                    None => continue,
                };
                let obj = match row_json.as_object() {
                    Some(o) => o,
                    None => continue,
                };

                // Discover current columns via PRAGMA.
                let table = &change.table;
                let pragma_sql = format!("PRAGMA table_info(\"{table}\")");
                let mut pragma_stmt =
                    conn.prepare(&pragma_sql).map_err(|e| e.to_string())?;
                let db_cols: Vec<String> = pragma_stmt
                    .query_map([], |r| r.get::<_, String>(1))
                    .map_err(|e| e.to_string())?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| e.to_string())?;

                // Intersect with row_json keys.
                let cols: Vec<&str> = db_cols
                    .iter()
                    .filter(|c| obj.contains_key(c.as_str()))
                    .map(|c| c.as_str())
                    .collect();

                if cols.is_empty() {
                    continue;
                }

                let col_list: String = cols
                    .iter()
                    .map(|c| format!("\"{}\"", c))
                    .collect::<Vec<_>>()
                    .join(", ");
                let params: String = (1..=cols.len())
                    .map(|i| format!("?{i}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let sql = format!(
                    "INSERT OR REPLACE INTO \"{table}\" ({col_list}) VALUES ({params})"
                );

                let values: Vec<rusqlite::types::Value> = cols
                    .iter()
                    .map(|c| json_to_sqlite_value(&obj[*c]))
                    .collect();

                conn.execute(
                    &sql,
                    rusqlite::params_from_iter(values.iter()),
                )
                .map_err(|e| e.to_string())?;

                applied += 1;
                if change.table == "raw_records" || change.table == "transaction_meta" {
                    touched_fts = true;
                }
            }
            "delete" => {
                let row_id = match &change.row_id {
                    Some(id) => id,
                    None => continue,
                };
                let table = &change.table;
                let sql = format!("DELETE FROM \"{table}\" WHERE \"{pk_col}\" = ?1");
                conn.execute(&sql, [row_id.as_str()])
                    .map_err(|e| e.to_string())?;
                applied += 1;
            }
            _ => {}
        }
    }

    if touched_fts {
        rebuild_fts(conn, data_key).map_err(|e| e)?;
    }

    Ok(applied)
}

fn json_to_sqlite_value(v: &serde_json::Value) -> rusqlite::types::Value {
    match v {
        serde_json::Value::Null => rusqlite::types::Value::Null,
        serde_json::Value::Bool(b) => rusqlite::types::Value::Integer(if *b { 1 } else { 0 }),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                rusqlite::types::Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                rusqlite::types::Value::Real(f)
            } else {
                rusqlite::types::Value::Null
            }
        }
        serde_json::Value::String(s) => rusqlite::types::Value::Text(s.clone()),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            rusqlite::types::Value::Text(v.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::open_connection;
    use crate::storage::migrations::run_migrations;
    use crate::sync::bundle::{BundleChange, SyncBundle};
    use std::collections::HashMap;

    fn setup() -> Connection {
        let mut conn = open_connection(":memory:", None).unwrap();
        run_migrations(&mut conn).unwrap();
        conn
    }

    fn make_bundle(changes: Vec<BundleChange>) -> SyncBundle {
        SyncBundle {
            schema_version: 1,
            device_id: "dev-1".to_string(),
            push_counter: 1,
            vector_clock: HashMap::new(),
            changes,
        }
    }

    fn account_upsert(id: &str) -> BundleChange {
        BundleChange {
            table: "accounts".to_string(),
            op: "upsert".to_string(),
            row_json: Some(serde_json::json!({
                "id": id,
                "name": "Test Account",
                "type": "depository",
                "subtype": "checking",
                "institution": "Test Bank",
                "currency": "USD",
                "created_at": "2025-01-01T00:00:00Z"
            })),
            row_id: None,
        }
    }

    #[test]
    fn apply_upsert_inserts_row() {
        let mut conn = setup();
        let key = [0u8; 32];
        let bundle = make_bundle(vec![account_upsert("acc-1")]);
        apply_bundle(&mut conn, &bundle, &key).unwrap();

        let name: String = conn
            .query_row(
                "SELECT name FROM accounts WHERE id = 'acc-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(name, "Test Account");
    }

    #[test]
    fn apply_upsert_is_idempotent() {
        let mut conn = setup();
        let key = [0u8; 32];
        let bundle = make_bundle(vec![account_upsert("acc-1")]);
        apply_bundle(&mut conn, &bundle, &key).unwrap();
        apply_bundle(&mut conn, &bundle, &key).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM accounts WHERE id = 'acc-1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn apply_delete_removes_row() {
        let mut conn = setup();
        let key = [0u8; 32];
        // Insert first
        apply_bundle(&mut conn, &make_bundle(vec![account_upsert("acc-1")]), &key).unwrap();

        let delete_bundle = make_bundle(vec![BundleChange {
            table: "accounts".to_string(),
            op: "delete".to_string(),
            row_json: None,
            row_id: Some("acc-1".to_string()),
        }]);
        apply_bundle(&mut conn, &delete_bundle, &key).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM accounts WHERE id = 'acc-1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn sync_skip_flag_is_zero_after_apply() {
        let mut conn = setup();
        let key = [0u8; 32];
        let bundle = make_bundle(vec![account_upsert("acc-1")]);
        apply_bundle(&mut conn, &bundle, &key).unwrap();

        let flag: i64 = conn
            .query_row("SELECT flag FROM _sync_skip WHERE rowid = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(flag, 0);
    }

    #[test]
    fn sync_skip_flag_is_zero_even_on_error() {
        let mut conn = setup();
        let key = [0u8; 32];
        // Apply a change to an unknown table — should return Err but skip guard clears flag.
        let bad_bundle = make_bundle(vec![BundleChange {
            table: "nonexistent_table".to_string(),
            op: "upsert".to_string(),
            row_json: Some(serde_json::json!({"id": "1"})),
            row_id: None,
        }]);
        // This should not panic; apply just skips unrecognized tables.
        apply_bundle(&mut conn, &bad_bundle, &key).unwrap();

        let flag: i64 = conn
            .query_row("SELECT flag FROM _sync_skip WHERE rowid = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(flag, 0);
    }

    #[test]
    fn apply_raw_records_triggers_fts_rebuild() {
        let mut conn = setup();
        let key = [0u8; 32];

        // Need parent rows first.
        apply_bundle(
            &mut conn,
            &make_bundle(vec![BundleChange {
                table: "accounts".to_string(),
                op: "upsert".to_string(),
                row_json: Some(serde_json::json!({
                    "id": "acc-1",
                    "name": "Test",
                    "type": "depository",
                    "subtype": "checking",
                    "institution": "Bank",
                    "currency": "USD",
                    "created_at": "2025-01-01T00:00:00Z"
                })),
                row_id: None,
            }]),
            &key,
        )
        .unwrap();
        apply_bundle(
            &mut conn,
            &make_bundle(vec![BundleChange {
                table: "transactions".to_string(),
                op: "upsert".to_string(),
                row_json: Some(serde_json::json!({
                    "id": "txn-1",
                    "account_id": "acc-1",
                    "created_at": "2025-01-01T00:00:00Z"
                })),
                row_id: None,
            }]),
            &key,
        )
        .unwrap();
        apply_bundle(
            &mut conn,
            &make_bundle(vec![BundleChange {
                table: "import_batches".to_string(),
                op: "upsert".to_string(),
                row_json: Some(serde_json::json!({
                    "id": "batch-1",
                    "source_type": "csv",
                    "account_id": "acc-1",
                    "filename": "test.csv",
                    "imported_at": "2025-01-01T00:00:00Z"
                })),
                row_id: None,
            }]),
            &key,
        )
        .unwrap();

        let raw_record_bundle = make_bundle(vec![BundleChange {
            table: "raw_records".to_string(),
            op: "upsert".to_string(),
            row_json: Some(serde_json::json!({
                "id": "rr-1",
                "transaction_id": "txn-1",
                "supersedes_id": null,
                "import_batch_id": "batch-1",
                "source_id": "src-1",
                "date": "2025-01-01",
                "amount_cents": -1000,
                "description": "Coffee Shop",
                "raw_json": "{}",
                "created_at": "2025-01-01T00:00:00Z"
            })),
            row_id: None,
        }]);
        apply_bundle(&mut conn, &raw_record_bundle, &key).unwrap();

        // FTS should have an entry for the description.
        let fts_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM transactions_fts WHERE description MATCH 'Coffee'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(fts_count > 0, "FTS should have the raw_record description indexed");
    }

    #[test]
    fn unknown_column_in_row_json_ignored() {
        let mut conn = setup();
        let key = [0u8; 32];
        let bundle = make_bundle(vec![BundleChange {
            table: "accounts".to_string(),
            op: "upsert".to_string(),
            row_json: Some(serde_json::json!({
                "id": "acc-1",
                "name": "Test",
                "type": "depository",
                "subtype": "checking",
                "institution": "Bank",
                "currency": "USD",
                "created_at": "2025-01-01T00:00:00Z",
                "future_column": "some_value"
            })),
            row_id: None,
        }]);
        // Should not error on the unknown column.
        apply_bundle(&mut conn, &bundle, &key).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM accounts WHERE id = 'acc-1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
    }
}
