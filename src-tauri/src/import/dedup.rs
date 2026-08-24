use rusqlite::Connection;

pub enum DedupClass {
    New,
    Exact,
    Uncertain {
        existing_raw_record_id: String,
        existing_source_id: String,
    },
}

pub struct Candidate {
    pub source_id: String,
    pub date: String,
    pub amount_cents: i64,
    pub description: String,
    pub raw_json: String,
}

pub struct ClassifiedRecord {
    pub candidate: Candidate,
    pub class: DedupClass,
}

pub fn classify_candidates(
    conn: &Connection,
    account_id: &str,
    candidates: Vec<Candidate>,
) -> Vec<ClassifiedRecord> {
    candidates
        .into_iter()
        .map(|c| {
            let class = classify_one(conn, account_id, &c);
            ClassifiedRecord { candidate: c, class }
        })
        .collect()
}

fn classify_one(conn: &Connection, account_id: &str, c: &Candidate) -> DedupClass {
    // Exact: same source_id already exists for this account.
    let exact: bool = conn
        .query_row(
            "SELECT 1 FROM raw_records r
             JOIN transactions t ON t.id = r.transaction_id
             WHERE r.source_id = ?1 AND t.account_id = ?2
             LIMIT 1",
            rusqlite::params![c.source_id, account_id],
            |_| Ok(true),
        )
        .unwrap_or(false);

    if exact {
        return DedupClass::Exact;
    }

    // Uncertain: same (date, amount, description) for this account, different source_id.
    let uncertain = conn
        .query_row(
            "SELECT r.id, r.source_id FROM raw_records r
             JOIN transactions t ON t.id = r.transaction_id
             WHERE r.date = ?1 AND r.amount_cents = ?2 AND r.description = ?3
               AND t.account_id = ?4 AND r.source_id != ?5
             LIMIT 1",
            rusqlite::params![c.date, c.amount_cents, c.description, account_id, c.source_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .ok();

    match uncertain {
        Some((existing_raw_record_id, existing_source_id)) => DedupClass::Uncertain {
            existing_raw_record_id,
            existing_source_id,
        },
        None => DedupClass::New,
    }
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

    fn insert_record(conn: &Connection, account_id: &str, source_id: &str, date: &str, amount_cents: i64, description: &str) {
        let tx_id = Uuid::new_v4().to_string();
        let rr_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO transactions (id, account_id, created_at) VALUES (?1, ?2, datetime('now'))",
            rusqlite::params![tx_id, account_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO raw_records (id, transaction_id, supersedes_id, import_batch_id, source_id, date, amount_cents, description, raw_json, created_at)
             VALUES (?1, ?2, NULL, NULL, ?3, ?4, ?5, ?6, '{}', datetime('now'))",
            rusqlite::params![rr_id, tx_id, source_id, date, amount_cents, description],
        ).unwrap();
    }

    fn make_candidate(source_id: &str, date: &str, amount_cents: i64, description: &str) -> Candidate {
        Candidate {
            source_id: source_id.to_string(),
            date: date.to_string(),
            amount_cents,
            description: description.to_string(),
            raw_json: "{}".to_string(),
        }
    }

    #[test]
    fn empty_candidates_returns_empty() {
        let conn = setup();
        let result = classify_candidates(&conn, "acct-1", vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn new_record_when_no_existing() {
        let conn = setup();
        let candidates = vec![make_candidate("src-1", "2026-01-15", -4567, "Grocery Store")];
        let result = classify_candidates(&conn, "acct-1", candidates);
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0].class, DedupClass::New));
    }

    #[test]
    fn exact_dup_detected_by_source_id() {
        let conn = setup();
        let account_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO accounts (id, name, type, subtype, institution, currency, created_at)
             VALUES (?1, 'Checking', 'depository', 'checking', 'Bank', 'USD', datetime('now'))",
            rusqlite::params![account_id],
        ).unwrap();
        insert_record(&conn, &account_id, "src-abc", "2026-01-15", -4567, "Grocery Store");

        let candidates = vec![make_candidate("src-abc", "2026-01-15", -4567, "Grocery Store")];
        let result = classify_candidates(&conn, &account_id, candidates);
        assert!(matches!(result[0].class, DedupClass::Exact));
    }

    #[test]
    fn uncertain_detected_same_fields_different_source_id() {
        let conn = setup();
        let account_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO accounts (id, name, type, subtype, institution, currency, created_at)
             VALUES (?1, 'Checking', 'depository', 'checking', 'Bank', 'USD', datetime('now'))",
            rusqlite::params![account_id],
        ).unwrap();
        insert_record(&conn, &account_id, "src-original", "2026-01-15", -4567, "Grocery Store");

        let candidates = vec![make_candidate("src-different", "2026-01-15", -4567, "Grocery Store")];
        let result = classify_candidates(&conn, &account_id, candidates);
        match &result[0].class {
            DedupClass::Uncertain { existing_source_id, .. } => {
                assert_eq!(existing_source_id, "src-original");
            }
            _ => panic!("expected Uncertain"),
        }
    }

    #[test]
    fn exact_takes_priority_over_uncertain() {
        // A candidate that matches both exact (source_id) and would match uncertain
        // should be classified Exact.
        let conn = setup();
        let account_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO accounts (id, name, type, subtype, institution, currency, created_at)
             VALUES (?1, 'Checking', 'depository', 'checking', 'Bank', 'USD', datetime('now'))",
            rusqlite::params![account_id],
        ).unwrap();
        insert_record(&conn, &account_id, "src-abc", "2026-01-15", -4567, "Grocery Store");

        // Same source_id, same fields - should be Exact not Uncertain
        let candidates = vec![make_candidate("src-abc", "2026-01-15", -4567, "Grocery Store")];
        let result = classify_candidates(&conn, &account_id, candidates);
        assert!(matches!(result[0].class, DedupClass::Exact));
    }

    #[test]
    fn same_fields_different_account_is_new() {
        // Dedup is scoped per account; same transaction on a different account is New.
        let conn = setup();
        let account_a = Uuid::new_v4().to_string();
        let account_b = Uuid::new_v4().to_string();
        for id in [&account_a, &account_b] {
            conn.execute(
                "INSERT INTO accounts (id, name, type, subtype, institution, currency, created_at)
                 VALUES (?1, 'Checking', 'depository', 'checking', 'Bank', 'USD', datetime('now'))",
                rusqlite::params![id],
            ).unwrap();
        }
        insert_record(&conn, &account_a, "src-a", "2026-01-15", -4567, "Grocery Store");

        // Importing the same transaction into account_b -> should be New
        let candidates = vec![make_candidate("src-b", "2026-01-15", -4567, "Grocery Store")];
        let result = classify_candidates(&conn, &account_b, candidates);
        assert!(matches!(result[0].class, DedupClass::New));
    }

    #[test]
    fn mixed_batch_classifies_correctly() {
        let conn = setup();
        let account_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO accounts (id, name, type, subtype, institution, currency, created_at)
             VALUES (?1, 'Checking', 'depository', 'checking', 'Bank', 'USD', datetime('now'))",
            rusqlite::params![account_id],
        ).unwrap();
        insert_record(&conn, &account_id, "src-exact", "2026-01-10", -1000, "Coffee");
        insert_record(&conn, &account_id, "src-uncertain-orig", "2026-01-11", -2000, "Gas");

        let candidates = vec![
            make_candidate("src-exact", "2026-01-10", -1000, "Coffee"),          // Exact
            make_candidate("src-uncertain-new", "2026-01-11", -2000, "Gas"),     // Uncertain
            make_candidate("src-brand-new", "2026-01-12", -3000, "Restaurant"),  // New
        ];
        let result = classify_candidates(&conn, &account_id, candidates);
        assert!(matches!(result[0].class, DedupClass::Exact));
        assert!(matches!(result[1].class, DedupClass::Uncertain { .. }));
        assert!(matches!(result[2].class, DedupClass::New));
    }
}
