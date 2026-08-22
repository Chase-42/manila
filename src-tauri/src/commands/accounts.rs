use std::sync::Mutex;
use tauri::State;
use rusqlite::Connection;
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct AccountRow {
    pub id: String,
    pub name: String,
    pub account_type: String,
    pub subtype: String,
    pub institution: String,
    pub currency: String,
    pub created_at: String,
}

fn list_accounts_inner(conn: &Connection) -> Result<Vec<AccountRow>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, name, type AS account_type, subtype, institution, currency, created_at
         FROM accounts
         ORDER BY name ASC",
    )?;

    let rows: Result<Vec<AccountRow>, _> = stmt
        .query_map([], |row| {
            Ok(AccountRow {
                id: row.get(0)?,
                name: row.get(1)?,
                account_type: row.get(2)?,
                subtype: row.get(3)?,
                institution: row.get(4)?,
                currency: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?
        .collect();

    rows
}

fn create_account_inner(
    conn: &Connection,
    name: &str,
    account_type: &str,
    subtype: &str,
    institution: &str,
    currency: &str,
) -> Result<String, String> {
    if name.trim().is_empty() {
        return Err("account name must not be blank".into());
    }

    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO accounts (id, name, type, subtype, institution, currency, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))",
        rusqlite::params![id, name.trim(), account_type, subtype, institution, currency],
    )
    .map_err(|e| e.to_string())?;

    Ok(id)
}

fn update_account_inner(
    conn: &Connection,
    id: &str,
    name: &str,
    account_type: &str,
    subtype: &str,
    institution: &str,
) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("account name must not be blank".into());
    }

    let rows_affected = conn
        .execute(
            "UPDATE accounts SET name = ?1, type = ?2, subtype = ?3, institution = ?4
             WHERE id = ?5",
            rusqlite::params![name.trim(), account_type, subtype, institution, id],
        )
        .map_err(|e| e.to_string())?;

    if rows_affected == 0 {
        return Err(format!("no account found with id {id}"));
    }

    Ok(())
}

#[tauri::command]
pub fn list_accounts(db: State<Mutex<Connection>>) -> Result<Vec<AccountRow>, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    list_accounts_inner(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_account(
    db: State<Mutex<Connection>>,
    name: String,
    account_type: String,
    subtype: String,
    institution: String,
    currency: String,
) -> Result<String, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    create_account_inner(&conn, &name, &account_type, &subtype, &institution, &currency)
}

#[tauri::command]
pub fn update_account(
    db: State<Mutex<Connection>>,
    id: String,
    name: String,
    account_type: String,
    subtype: String,
    institution: String,
) -> Result<(), String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    update_account_inner(&conn, &id, &name, &account_type, &subtype, &institution)
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

    #[test]
    fn create_then_list_round_trip() {
        let conn = setup();

        let id = create_account_inner(&conn, "Checking", "depository", "checking", "Chase", "USD")
            .unwrap();

        let accounts = list_accounts_inner(&conn).unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].id, id);
        assert_eq!(accounts[0].name, "Checking");
        assert_eq!(accounts[0].account_type, "depository");
        assert_eq!(accounts[0].currency, "USD");
    }

    #[test]
    fn update_persists_changes() {
        let conn = setup();

        let id =
            create_account_inner(&conn, "Old Name", "depository", "checking", "Chase", "USD")
                .unwrap();

        update_account_inner(&conn, &id, "New Name", "credit", "credit card", "Amex").unwrap();

        let accounts = list_accounts_inner(&conn).unwrap();
        assert_eq!(accounts[0].name, "New Name");
        assert_eq!(accounts[0].account_type, "credit");
        assert_eq!(accounts[0].institution, "Amex");
    }

    #[test]
    fn blank_name_returns_err_on_create() {
        let conn = setup();
        let result = create_account_inner(&conn, "  ", "depository", "checking", "Chase", "USD");
        assert!(result.is_err());
    }

    #[test]
    fn blank_name_returns_err_on_update() {
        let conn = setup();
        let id =
            create_account_inner(&conn, "Checking", "depository", "checking", "Chase", "USD")
                .unwrap();
        let result = update_account_inner(&conn, &id, "", "depository", "checking", "Chase");
        assert!(result.is_err());
    }
}
