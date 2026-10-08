use crate::crypto::field::seal_field;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct PlaidConnectionRow {
    pub id: String,
    pub plaid_item_id: String,
    pub institution_name: String,
    pub institution_id: String,
    pub account_ids: String,
    pub connected_at: String,
    pub last_synced_at: Option<String>,
}

fn list_plaid_connections_inner(
    conn: &Connection,
) -> Result<Vec<PlaidConnectionRow>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, plaid_item_id, institution_name, institution_id,
                account_ids, connected_at, last_synced_at
         FROM plaid_connections
         ORDER BY connected_at DESC",
    )?;

    let rows: Result<Vec<PlaidConnectionRow>, _> = stmt
        .query_map([], |row| {
            Ok(PlaidConnectionRow {
                id: row.get(0)?,
                plaid_item_id: row.get(1)?,
                institution_name: row.get(2)?,
                institution_id: row.get(3)?,
                account_ids: row.get(4)?,
                connected_at: row.get(5)?,
                last_synced_at: row.get(6)?,
            })
        })?
        .collect();
    rows
}

fn upsert_plaid_connection_inner(
    conn: &Connection,
    key: &[u8; 32],
    plaid_item_id: &str,
    access_token: &str,
    institution_name: &str,
    institution_id: &str,
    account_ids: &str,
) -> Result<String, String> {
    if plaid_item_id.trim().is_empty() {
        return Err("plaid_item_id must not be blank".into());
    }
    if access_token.trim().is_empty() {
        return Err("access_token must not be blank".into());
    }

    let sealed = seal_field(key, access_token);

    let existing_id: Option<String> = conn
        .query_row(
            "SELECT id FROM plaid_connections WHERE plaid_item_id = ?1",
            [plaid_item_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;

    if let Some(id) = existing_id {
        conn.execute(
            "UPDATE plaid_connections
             SET access_token = ?1, institution_name = ?2, institution_id = ?3, account_ids = ?4
             WHERE plaid_item_id = ?5",
            rusqlite::params![sealed, institution_name, institution_id, account_ids, plaid_item_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(id)
    } else {
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO plaid_connections
             (id, plaid_item_id, access_token, institution_name, institution_id, account_ids, connected_at, last_synced_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'), NULL)",
            rusqlite::params![
                id,
                plaid_item_id,
                sealed,
                institution_name,
                institution_id,
                account_ids
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(id)
    }
}

fn delete_plaid_connection_inner(conn: &Connection, id: &str) -> Result<(), String> {
    let rows_affected = conn
        .execute(
            "DELETE FROM plaid_connections WHERE id = ?1",
            [id],
        )
        .map_err(|e| e.to_string())?;

    if rows_affected == 0 {
        return Err(format!("no plaid connection found with id {id}"));
    }
    Ok(())
}

#[tauri::command]
pub fn list_plaid_connections(
    vault: State<'_, crate::crypto::VaultState>,
) -> Result<Vec<PlaidConnectionRow>, String> {
    let guard = vault
        .0
        .lock()
        .map_err(|_| "vault lock poisoned".to_string())?;
    let unlocked = guard.as_ref().ok_or("locked")?;
    list_plaid_connections_inner(&unlocked.conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn upsert_plaid_connection(
    vault: State<'_, crate::crypto::VaultState>,
    plaid_item_id: String,
    access_token: String,
    institution_name: String,
    institution_id: String,
    account_ids: String,
) -> Result<String, String> {
    let guard = vault
        .0
        .lock()
        .map_err(|_| "vault lock poisoned".to_string())?;
    let unlocked = guard.as_ref().ok_or("locked")?;
    upsert_plaid_connection_inner(
        &unlocked.conn,
        &unlocked.keys.data_key,
        &plaid_item_id,
        &access_token,
        &institution_name,
        &institution_id,
        &account_ids,
    )
}

#[tauri::command]
pub fn delete_plaid_connection(
    vault: State<'_, crate::crypto::VaultState>,
    id: String,
) -> Result<(), String> {
    let guard = vault
        .0
        .lock()
        .map_err(|_| "vault lock poisoned".to_string())?;
    let unlocked = guard.as_ref().ok_or("locked")?;
    delete_plaid_connection_inner(&unlocked.conn, &id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::field::open_field;
    use crate::storage::db::open_connection;
    use crate::storage::migrations::run_migrations;

    fn setup() -> Connection {
        let mut conn = open_connection(":memory:", None).unwrap();
        run_migrations(&mut conn).unwrap();
        conn
    }

    const KEY: [u8; 32] = [0x42u8; 32];

    #[test]
    fn list_empty_returns_empty_vec() {
        let conn = setup();
        let rows = list_plaid_connections_inner(&conn).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn upsert_then_list_round_trip() {
        let conn = setup();
        let id = upsert_plaid_connection_inner(
            &conn,
            &KEY,
            "item_abc",
            "access-sandbox-token",
            "Chase",
            "ins_3",
            r#"["acc_1","acc_2"]"#,
        )
        .unwrap();

        let rows = list_plaid_connections_inner(&conn).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, id);
        assert_eq!(rows[0].plaid_item_id, "item_abc");
        assert_eq!(rows[0].institution_name, "Chase");
        assert!(rows[0].last_synced_at.is_none());

        // Verify the stored access_token is encrypted and decrypts back correctly.
        let raw: String = conn
            .query_row(
                "SELECT access_token FROM plaid_connections WHERE id = ?1",
                [&id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(raw.starts_with("v1:"), "stored token must be encrypted");
        assert_eq!(open_field(&KEY, &raw).unwrap(), "access-sandbox-token");
    }

    #[test]
    fn upsert_same_item_id_updates_not_duplicates() {
        let conn = setup();
        let id1 = upsert_plaid_connection_inner(
            &conn,
            &KEY,
            "item_abc",
            "token-v1",
            "Chase",
            "ins_3",
            r#"["acc_1"]"#,
        )
        .unwrap();

        let id2 = upsert_plaid_connection_inner(
            &conn,
            &KEY,
            "item_abc",
            "token-v2",
            "Chase",
            "ins_3",
            r#"["acc_1","acc_2"]"#,
        )
        .unwrap();

        assert_eq!(id1, id2, "id must be stable across re-links");

        let rows = list_plaid_connections_inner(&conn).unwrap();
        assert_eq!(rows.len(), 1, "must not insert a duplicate row");
        assert_eq!(rows[0].account_ids, r#"["acc_1","acc_2"]"#);

        // Verify the new token was encrypted and stored.
        let raw: String = conn
            .query_row(
                "SELECT access_token FROM plaid_connections WHERE id = ?1",
                [&id1],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(open_field(&KEY, &raw).unwrap(), "token-v2");
    }

    #[test]
    fn delete_removes_row() {
        let conn = setup();
        let id = upsert_plaid_connection_inner(
            &conn,
            &KEY,
            "item_abc",
            "token",
            "Chase",
            "ins_3",
            "[]",
        )
        .unwrap();

        delete_plaid_connection_inner(&conn, &id).unwrap();
        let rows = list_plaid_connections_inner(&conn).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn delete_missing_id_returns_err() {
        let conn = setup();
        let result = delete_plaid_connection_inner(&conn, "nonexistent-id");
        assert!(result.is_err());
    }

    #[test]
    fn blank_plaid_item_id_returns_err() {
        let conn = setup();
        let result = upsert_plaid_connection_inner(&conn, &KEY, "  ", "token", "Chase", "ins_3", "[]");
        assert!(result.is_err());
    }

    #[test]
    fn blank_access_token_returns_err() {
        let conn = setup();
        let result = upsert_plaid_connection_inner(&conn, &KEY, "item_abc", "", "Chase", "ins_3", "[]");
        assert!(result.is_err());
    }

    #[test]
    fn lock_gate_rejects_none_vault() {
        use crate::crypto::VaultState;
        use std::sync::Mutex;
        let vault = VaultState(Mutex::new(None));
        assert_eq!(
            crate::commands::require_unlocked(&vault).unwrap_err(),
            "locked"
        );
    }
}
