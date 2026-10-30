use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::commands::import::commit_records;
use crate::commands::search::rebuild_fts;
use crate::crypto::field::open_field;
use crate::import::dedup::{self, Candidate};
use crate::types::Cents;

#[derive(Debug, Deserialize)]
pub struct PlaidBalances {
    pub iso_currency_code: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PlaidAccountObject {
    pub account_id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub account_type: String,
    pub subtype: Option<String>,
    pub balances: PlaidBalances,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlaidTransactionObject {
    pub transaction_id: String,
    pub account_id: String,
    pub date: String,
    pub amount: f64,
    pub name: String,
    pub merchant_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PlaidSyncPage {
    pub accounts: Vec<PlaidAccountObject>,
    pub added: Vec<PlaidTransactionObject>,
    pub next_cursor: String,
    pub has_more: bool,
}


#[derive(Debug, Serialize)]
pub struct SyncResult {
    pub imported_count: u32,
    pub accounts_created: u32,
    pub skipped_count: u32,
    pub errors: Vec<String>,
}


pub(crate) fn map_plaid_type(plaid_type: &str) -> &'static str {
    match plaid_type {
        "credit" => "credit",
        "investment" | "brokerage" => "investment",
        _ => "depository",
    }
}

pub(crate) fn plaid_amount_to_cents(amount: f64) -> Cents {
    Cents((-(amount * 100.0).round()) as i64)
}

pub(crate) fn plaid_description(t: &PlaidTransactionObject) -> String {
    match &t.merchant_name {
        Some(m) if !m.is_empty() => m.clone(),
        _ => t.name.clone(),
    }
}


pub(crate) fn sync_plaid_connection_inner(
    conn: &mut Connection,
    key: &[u8; 32],
    pages: Vec<PlaidSyncPage>,
    institution_name: &str,
) -> Result<SyncResult, String> {
    // Build a deduped Plaid account_id → local account UUID map.
    let mut plaid_to_local: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut accounts_created: u32 = 0;

    for page in &pages {
        for acct in &page.accounts {
            if plaid_to_local.contains_key(&acct.account_id) {
                continue;
            }

            let existing_id: Option<String> = conn
                .query_row(
                    "SELECT id FROM accounts WHERE plaid_account_id = ?1",
                    [&acct.account_id],
                    |row| row.get(0),
                )
                .ok();

            let local_id = if let Some(id) = existing_id {
                id
            } else {
                let new_id = Uuid::new_v4().to_string();
                let acct_type = map_plaid_type(&acct.account_type);
                let subtype = acct.subtype.as_deref().unwrap_or("checking");
                let currency = acct
                    .balances
                    .iso_currency_code
                    .as_deref()
                    .unwrap_or("USD");
                conn.execute(
                    "INSERT INTO accounts
                     (id, name, type, subtype, institution, currency, created_at, plaid_account_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'), ?7)",
                    rusqlite::params![
                        new_id,
                        acct.name,
                        acct_type,
                        subtype,
                        institution_name,
                        currency,
                        acct.account_id,
                    ],
                )
                .map_err(|e| e.to_string())?;
                accounts_created += 1;
                new_id
            };

            plaid_to_local.insert(acct.account_id.clone(), local_id);
        }
    }

    if plaid_to_local.is_empty() {
        return Ok(SyncResult {
            imported_count: 0,
            accounts_created: 0,
            skipped_count: 0,
            errors: vec![],
        });
    }

    // Group added transactions by local account UUID.
    let mut groups: std::collections::HashMap<String, Vec<Candidate>> =
        std::collections::HashMap::new();

    for page in &pages {
        for txn in &page.added {
            let local_id = match plaid_to_local.get(&txn.account_id) {
                Some(id) => id.clone(),
                None => continue,
            };
            let source_id = format!("plaid|{}|{}", local_id, txn.transaction_id);
            let description = plaid_description(txn);
            let raw_json = serde_json::to_string(txn).unwrap_or_default();
            groups.entry(local_id).or_default().push(Candidate {
                source_id,
                date: txn.date.clone(),
                amount_cents: plaid_amount_to_cents(txn.amount),
                description,
                raw_json,
            });
        }
    }

    let mut imported_count: u32 = 0;
    let mut skipped_count: u32 = 0;

    for (local_account_id, candidates) in groups {
        let classified =
            dedup::classify_candidates(conn, key, &local_account_id, candidates)?;
        let result = commit_records(
            conn,
            key,
            "plaid",
            &local_account_id,
            "",
            classified,
            &[],
            vec![],
        )?;
        imported_count += result.imported_count;
        skipped_count += result.skipped_count;
    }

    if imported_count > 0 {
        rebuild_fts(conn, key)?;
    }

    Ok(SyncResult {
        imported_count,
        accounts_created,
        skipped_count,
        errors: vec![],
    })
}


#[tauri::command]
pub async fn sync_plaid_connection(
    vault: tauri::State<'_, crate::crypto::VaultState>,
    connection_id: String,
    proxy_url: String,
) -> Result<SyncResult, String> {
    // Step 1: Read everything we need from the vault before any .await.
    let (data_key, access_token_sealed, cursor_opt, institution_name) = {
        let guard = vault
            .0
            .lock()
            .map_err(|_| "vault lock poisoned".to_string())?;
        let unlocked = guard.as_ref().ok_or("locked")?;

        let row: (String, Option<String>, String) = unlocked
            .conn
            .query_row(
                "SELECT access_token, last_synced_at, institution_name
                 FROM plaid_connections WHERE id = ?1",
                [&connection_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|e| e.to_string())?;

        (unlocked.keys.data_key, row.0, row.1, row.2)
    }; // mutex released here

    // Step 2: Decrypt the access token.
    let access_token = open_field(&data_key, &access_token_sealed)
        .map_err(|e| e.to_string())?;

    // Step 3: Paginate through /plaid/transactions.
    let http = reqwest::Client::new();
    let mut pages: Vec<PlaidSyncPage> = Vec::new();
    let mut current_cursor = cursor_opt;

    loop {
        let mut body = serde_json::json!({ "access_token": access_token });
        if let Some(ref c) = current_cursor {
            body["cursor"] = serde_json::Value::String(c.clone());
        }

        let resp = http
            .post(format!("{}/plaid/transactions", proxy_url.trim_end_matches('/')))
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("proxy error {status}: {text}"));
        }

        let page: PlaidSyncPage = resp.json().await.map_err(|e| e.to_string())?;
        let has_more = page.has_more;
        let next_cursor = page.next_cursor.clone();
        pages.push(page);

        if !has_more {
            current_cursor = Some(next_cursor);
            break;
        }
        current_cursor = Some(next_cursor);
    }

    let final_cursor = current_cursor.unwrap_or_default();

    // Step 4: Re-acquire the vault mutex for all DB writes.
    let mut guard = vault
        .0
        .lock()
        .map_err(|_| "vault lock poisoned".to_string())?;
    let unlocked = guard.as_mut().ok_or("locked")?;

    let result = sync_plaid_connection_inner(
        &mut unlocked.conn,
        &data_key,
        pages,
        &institution_name,
    )?;

    // Update the cursor checkpoint.
    unlocked
        .conn
        .execute(
            "UPDATE plaid_connections SET last_synced_at = ?1 WHERE id = ?2",
            rusqlite::params![final_cursor, connection_id],
        )
        .map_err(|e| e.to_string())?;

    Ok(result)
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::open_connection;
    use crate::storage::migrations::run_migrations;
    use crate::types::Cents;

    fn setup() -> Connection {
        let mut conn = open_connection(":memory:", None).unwrap();
        run_migrations(&mut conn).unwrap();
        conn
    }

    const KEY: [u8; 32] = [0x42u8; 32];

    fn insert_plaid_connection(conn: &Connection) -> String {
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO plaid_connections
             (id, plaid_item_id, access_token, institution_name, institution_id, account_ids, connected_at)
             VALUES (?1, 'item-1', 'v1:fake', 'Chase', 'ins_1', '[]', datetime('now'))",
            rusqlite::params![id],
        )
        .unwrap();
        id
    }

    fn make_page(
        plaid_acct_id: &str,
        transactions: &[(&str, f64)],
        next_cursor: &str,
        has_more: bool,
    ) -> PlaidSyncPage {
        PlaidSyncPage {
            accounts: vec![PlaidAccountObject {
                account_id: plaid_acct_id.to_string(),
                name: "Checking".to_string(),
                account_type: "depository".to_string(),
                subtype: Some("checking".to_string()),
                balances: PlaidBalances {
                    iso_currency_code: Some("USD".to_string()),
                },
            }],
            added: transactions
                .iter()
                .map(|(tid, amt)| PlaidTransactionObject {
                    transaction_id: tid.to_string(),
                    account_id: plaid_acct_id.to_string(),
                    date: "2026-01-15".to_string(),
                    amount: *amt,
                    name: "Coffee Shop".to_string(),
                    merchant_name: None,
                })
                .collect(),
            next_cursor: next_cursor.to_string(),
            has_more,
        }
    }


    #[test]
    fn amount_positive_is_outflow() {
        assert_eq!(plaid_amount_to_cents(12.50), Cents(-1250));
    }

    #[test]
    fn amount_negative_is_inflow() {
        assert_eq!(plaid_amount_to_cents(-5.00), Cents(500));
    }

    #[test]
    fn amount_zero() {
        assert_eq!(plaid_amount_to_cents(0.0), Cents(0));
    }

    #[test]
    fn amount_fractional_rounds() {
        // 12.005 * 100 = 1200.5, round → 1201, negate → -1201
        assert_eq!(plaid_amount_to_cents(12.005), Cents(-1201));
    }


    #[test]
    fn type_depository() {
        assert_eq!(map_plaid_type("depository"), "depository");
    }

    #[test]
    fn type_credit() {
        assert_eq!(map_plaid_type("credit"), "credit");
    }

    #[test]
    fn type_investment() {
        assert_eq!(map_plaid_type("investment"), "investment");
    }

    #[test]
    fn type_brokerage() {
        assert_eq!(map_plaid_type("brokerage"), "investment");
    }

    #[test]
    fn type_loan() {
        assert_eq!(map_plaid_type("loan"), "depository");
    }

    #[test]
    fn type_other() {
        assert_eq!(map_plaid_type("other"), "depository");
    }

    #[test]
    fn type_unknown() {
        assert_eq!(map_plaid_type("xyz"), "depository");
    }


    #[test]
    fn description_uses_merchant_name_when_present() {
        let txn = PlaidTransactionObject {
            transaction_id: "t1".into(),
            account_id: "a1".into(),
            date: "2026-01-01".into(),
            amount: 5.0,
            name: "COFFEE SHOP #12".into(),
            merchant_name: Some("Starbucks".into()),
        };
        assert_eq!(plaid_description(&txn), "Starbucks");
    }

    #[test]
    fn description_falls_back_to_name_when_merchant_none() {
        let txn = PlaidTransactionObject {
            transaction_id: "t1".into(),
            account_id: "a1".into(),
            date: "2026-01-01".into(),
            amount: 5.0,
            name: "COFFEE SHOP #12".into(),
            merchant_name: None,
        };
        assert_eq!(plaid_description(&txn), "COFFEE SHOP #12");
    }

    #[test]
    fn description_falls_back_to_name_when_merchant_empty() {
        let txn = PlaidTransactionObject {
            transaction_id: "t1".into(),
            account_id: "a1".into(),
            date: "2026-01-01".into(),
            amount: 5.0,
            name: "COFFEE SHOP #12".into(),
            merchant_name: Some(String::new()),
        };
        assert_eq!(plaid_description(&txn), "COFFEE SHOP #12");
    }


    #[test]
    fn new_account_and_transactions_are_created() {
        let mut conn = setup();
        let _conn_id = insert_plaid_connection(&conn);

        let page = make_page("plaid-acct-1", &[("txn-1", 12.50), ("txn-2", 5.00)], "cursor-1", false);

        let result = sync_plaid_connection_inner(&mut conn, &KEY, vec![page], "Chase").unwrap();

        assert_eq!(result.accounts_created, 1);
        assert_eq!(result.imported_count, 2);
        assert_eq!(result.skipped_count, 0);

        // The local account was created with plaid_account_id set.
        let plaid_acct_id: String = conn
            .query_row(
                "SELECT plaid_account_id FROM accounts WHERE plaid_account_id = 'plaid-acct-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(plaid_acct_id, "plaid-acct-1");

        // Source IDs follow the expected format.
        let local_id: String = conn
            .query_row(
                "SELECT id FROM accounts WHERE plaid_account_id = 'plaid-acct-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        let source_ids: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT source_id FROM raw_records ORDER BY source_id")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        };
        assert_eq!(source_ids[0], format!("plaid|{}|txn-1", local_id));
        assert_eq!(source_ids[1], format!("plaid|{}|txn-2", local_id));
    }

    #[test]
    fn re_sync_same_transactions_skips_all() {
        let mut conn = setup();
        let _conn_id = insert_plaid_connection(&conn);

        let page = make_page("plaid-acct-1", &[("txn-1", 12.50), ("txn-2", 5.00)], "cursor-1", false);
        let page2 = make_page("plaid-acct-1", &[("txn-1", 12.50), ("txn-2", 5.00)], "cursor-2", false);

        sync_plaid_connection_inner(&mut conn, &KEY, vec![page], "Chase").unwrap();
        let result2 = sync_plaid_connection_inner(&mut conn, &KEY, vec![page2], "Chase").unwrap();

        assert_eq!(result2.imported_count, 0);
        assert_eq!(result2.skipped_count, 2);
        assert_eq!(result2.accounts_created, 0); // account already exists
    }

    #[test]
    fn two_plaid_accounts_create_two_local_accounts() {
        let mut conn = setup();

        let page = PlaidSyncPage {
            accounts: vec![
                PlaidAccountObject {
                    account_id: "plaid-acct-a".into(),
                    name: "Checking".into(),
                    account_type: "depository".into(),
                    subtype: Some("checking".into()),
                    balances: PlaidBalances { iso_currency_code: Some("USD".into()) },
                },
                PlaidAccountObject {
                    account_id: "plaid-acct-b".into(),
                    name: "Savings".into(),
                    account_type: "depository".into(),
                    subtype: Some("savings".into()),
                    balances: PlaidBalances { iso_currency_code: Some("USD".into()) },
                },
            ],
            added: vec![
                PlaidTransactionObject {
                    transaction_id: "txn-a".into(),
                    account_id: "plaid-acct-a".into(),
                    date: "2026-01-15".into(),
                    amount: 10.0,
                    name: "Store A".into(),
                    merchant_name: None,
                },
                PlaidTransactionObject {
                    transaction_id: "txn-b".into(),
                    account_id: "plaid-acct-b".into(),
                    date: "2026-01-15".into(),
                    amount: 20.0,
                    name: "Store B".into(),
                    merchant_name: None,
                },
            ],
            next_cursor: "cursor-1".into(),
            has_more: false,
        };

        let result = sync_plaid_connection_inner(&mut conn, &KEY, vec![page], "Chase").unwrap();

        assert_eq!(result.accounts_created, 2);
        assert_eq!(result.imported_count, 2);

        let acct_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM accounts WHERE plaid_account_id IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(acct_count, 2);
    }

    #[test]
    fn empty_pages_returns_zero_counts() {
        let mut conn = setup();

        let page = PlaidSyncPage {
            accounts: vec![],
            added: vec![],
            next_cursor: "cursor-1".into(),
            has_more: false,
        };

        let result = sync_plaid_connection_inner(&mut conn, &KEY, vec![page], "Chase").unwrap();
        assert_eq!(result.imported_count, 0);
        assert_eq!(result.accounts_created, 0);
        assert_eq!(result.skipped_count, 0);
    }
}
