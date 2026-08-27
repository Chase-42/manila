use rusqlite::Connection;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub(crate) enum EngineError {
    #[error("postings must not be empty")]
    EmptyPostings,
    #[error("postings do not balance: sum is {0} cents (must be zero)")]
    UnbalancedPostings(i64),
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
}

pub(crate) struct PostingInput {
    pub account_id: String,
    pub amount_cents: i64,
}

/// Create a balanced double-entry transaction.
///
/// Sign convention (defined once here, applied everywhere):
///   negative amount_cents = outflow (money leaving the account)
///   positive amount_cents = inflow (money arriving at the account)
///
/// Every call must supply postings that sum to exactly zero. This is asserted
/// before any write; an imbalance or empty slice returns Err and writes nothing.
pub(crate) fn create_transaction(
    conn: &mut Connection,
    account_id: &str,
    date: &str,
    amount_cents: i64,
    description: &str,
    postings: &[PostingInput],
) -> Result<String, EngineError> {
    if postings.is_empty() {
        return Err(EngineError::EmptyPostings);
    }

    // Sign convention: negative = outflow, positive = inflow. Sum must be zero.
    let sum: i64 = postings.iter().map(|p| p.amount_cents).sum();
    if sum != 0 {
        return Err(EngineError::UnbalancedPostings(sum));
    }

    let transaction_id = Uuid::new_v4().to_string();
    let raw_record_id = Uuid::new_v4().to_string();
    let source_id = Uuid::new_v4().to_string();

    let tx = conn.transaction()?;

    tx.execute(
        "INSERT INTO transactions (id, account_id, created_at)
         VALUES (?1, ?2, datetime('now'))",
        rusqlite::params![transaction_id, account_id],
    )?;

    tx.execute(
        "INSERT INTO raw_records
             (id, transaction_id, supersedes_id, import_batch_id,
              source_id, date, amount_cents, description, raw_json, created_at)
         VALUES (?1, ?2, NULL, NULL, ?3, ?4, ?5, ?6, '{}', datetime('now'))",
        rusqlite::params![
            raw_record_id,
            transaction_id,
            source_id,
            date,
            amount_cents,
            description
        ],
    )?;

    for posting in postings {
        let posting_id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO postings (id, transaction_id, account_id, amount_cents)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                posting_id,
                transaction_id,
                posting.account_id,
                posting.amount_cents
            ],
        )?;
    }

    tx.commit()?;
    Ok(transaction_id)
}

#[derive(Debug, Error)]
pub(crate) enum TransferError {
    #[error("a transfer requires exactly two postings, got {0}")]
    WrongPostingCount(usize),
    #[error("a transfer must touch two different accounts")]
    SameAccount,
    #[error(transparent)]
    Engine(#[from] EngineError),
}

/// Create a transfer between two owned accounts.
///
/// A transfer is a transaction whose postings touch exactly two different
/// accounts and sum to zero. No category assignment is involved.
pub(crate) fn create_transfer(
    conn: &mut Connection,
    account_id: &str,
    date: &str,
    amount_cents: i64,
    description: &str,
    postings: &[PostingInput],
) -> Result<String, TransferError> {
    if postings.len() != 2 {
        return Err(TransferError::WrongPostingCount(postings.len()));
    }
    if postings[0].account_id == postings[1].account_id {
        return Err(TransferError::SameAccount);
    }
    Ok(create_transaction(
        conn,
        account_id,
        date,
        amount_cents,
        description,
        postings,
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::open_connection;
    use crate::storage::migrations::run_migrations;

    fn setup() -> Connection {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute_batch(
            "INSERT INTO accounts (id, name, type, subtype, institution, currency, created_at)
             VALUES ('acc-checking', 'Checking', 'depository', 'checking', 'Test Bank', 'USD', datetime('now'));
             INSERT INTO accounts (id, name, type, subtype, institution, currency, created_at)
             VALUES ('acc-credit', 'Credit Card', 'credit', 'credit card', 'Test Bank', 'USD', datetime('now'));",
        )
        .unwrap();
        conn
    }

    #[test]
    fn balanced_postings_succeed_and_rows_are_written() {
        let mut conn = setup();
        let postings = vec![
            PostingInput {
                account_id: "acc-checking".into(),
                amount_cents: -5000,
            },
            PostingInput {
                account_id: "acc-credit".into(),
                amount_cents: 5000,
            },
        ];

        let tx_id = create_transaction(
            &mut conn,
            "acc-checking",
            "2026-08-22",
            -5000,
            "TRADER JOE'S",
            &postings,
        )
        .unwrap();

        let tx_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM transactions WHERE id = ?1",
                [&tx_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tx_count, 1);

        let posting_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM postings WHERE transaction_id = ?1",
                [&tx_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(posting_count, 2);

        let rr_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM raw_records WHERE transaction_id = ?1",
                [&tx_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rr_count, 1);
    }

    #[test]
    fn unbalanced_postings_return_err_and_write_nothing() {
        let mut conn = setup();
        let postings = vec![
            PostingInput {
                account_id: "acc-checking".into(),
                amount_cents: -5000,
            },
            PostingInput {
                account_id: "acc-credit".into(),
                amount_cents: 4999,
            },
        ];

        let result = create_transaction(
            &mut conn,
            "acc-checking",
            "2026-08-22",
            -5000,
            "TRADER JOE'S",
            &postings,
        );

        assert!(matches!(result, Err(EngineError::UnbalancedPostings(-1))));

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM transactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "failed transaction must write nothing");
    }

    #[test]
    fn empty_postings_return_err() {
        let mut conn = setup();
        let result = create_transaction(&mut conn, "acc-checking", "2026-08-22", 0, "test", &[]);
        assert!(matches!(result, Err(EngineError::EmptyPostings)));
    }

    #[test]
    fn transfer_creates_two_postings_summing_to_zero() {
        let mut conn = setup();
        let postings = vec![
            PostingInput {
                account_id: "acc-checking".into(),
                amount_cents: -45000,
            },
            PostingInput {
                account_id: "acc-credit".into(),
                amount_cents: 45000,
            },
        ];

        let tx_id = create_transfer(
            &mut conn,
            "acc-checking",
            "2026-08-22",
            -45000,
            "AMEX AUTOPAY",
            &postings,
        )
        .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM postings WHERE transaction_id = ?1",
                [&tx_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);

        let sum: i64 = conn
            .query_row(
                "SELECT SUM(amount_cents) FROM postings WHERE transaction_id = ?1",
                [&tx_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(sum, 0);
    }
}
