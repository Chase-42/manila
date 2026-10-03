use crate::crypto::field::seal_field;
use rusqlite::Connection;

pub fn encrypt_existing_fields(
    conn: &mut Connection,
    key: &[u8; 32],
) -> Result<(), rusqlite::Error> {
    // Collect rows before opening the write transaction so the read cursors are
    // dropped and the connection is free to start an exclusive transaction.
    let raw_record_updates: Vec<(String, String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT id, description, raw_json FROM raw_records
             WHERE description NOT LIKE 'v1:%' OR raw_json NOT LIKE 'v1:%'",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut updates = Vec::new();
        for row in rows {
            updates.push(row?);
        }
        updates
    };

    let meta_updates: Vec<(String, Option<String>, String)> = {
        let mut stmt = conn.prepare(
            "SELECT transaction_id, notes, tags FROM transaction_meta
             WHERE (notes IS NOT NULL AND notes NOT LIKE 'v1:%')
                OR (tags IS NOT NULL AND tags NOT LIKE 'v1:%')",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut updates = Vec::new();
        for row in rows {
            updates.push(row?);
        }
        updates
    };

    if raw_record_updates.is_empty() && meta_updates.is_empty() {
        return Ok(());
    }

    let tx = conn.transaction()?;

    for (id, description, raw_json) in raw_record_updates {
        let new_description = if description.starts_with("v1:") {
            description
        } else {
            seal_field(key, &description)
        };
        let new_raw_json = if raw_json.starts_with("v1:") {
            raw_json
        } else {
            seal_field(key, &raw_json)
        };
        tx.execute(
            "UPDATE raw_records SET description = ?1, raw_json = ?2 WHERE id = ?3",
            rusqlite::params![new_description, new_raw_json, id],
        )?;
    }

    for (transaction_id, notes, tags) in meta_updates {
        let new_notes = match notes {
            Some(n) if !n.starts_with("v1:") => Some(seal_field(key, &n)),
            other => other,
        };
        let new_tags = if tags.starts_with("v1:") {
            tags
        } else {
            seal_field(key, &tags)
        };
        tx.execute(
            "UPDATE transaction_meta SET notes = ?1, tags = ?2 WHERE transaction_id = ?3",
            rusqlite::params![new_notes, new_tags, transaction_id],
        )?;
    }

    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::field::{open_field, seal_field};
    use crate::storage::db::open_connection;
    use crate::storage::migrations::run_migrations;
    use uuid::Uuid;

    fn setup() -> rusqlite::Connection {
        let mut conn = open_connection(":memory:", None).unwrap();
        run_migrations(&mut conn).unwrap();
        conn
    }

    fn insert_plaintext_record(
        conn: &Connection,
        description: &str,
        raw_json: &str,
    ) -> (String, String) {
        let tx_id = Uuid::new_v4().to_string();
        let rr_id = Uuid::new_v4().to_string();
        let acct_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO accounts (id, name, type, subtype, institution, currency, created_at)
             VALUES (?1, 'Bank', 'depository', 'checking', 'Bank', 'USD', datetime('now'))",
            rusqlite::params![acct_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transactions (id, account_id, created_at) VALUES (?1, ?2, datetime('now'))",
            rusqlite::params![tx_id, acct_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO raw_records
             (id, transaction_id, supersedes_id, import_batch_id, source_id,
              date, amount_cents, description, raw_json, created_at)
             VALUES (?1, ?2, NULL, NULL, ?3, '2026-01-15', -1000, ?4, ?5, datetime('now'))",
            rusqlite::params![
                rr_id,
                tx_id,
                format!("src|{}", rr_id),
                description,
                raw_json
            ],
        )
        .unwrap();
        (tx_id, rr_id)
    }

    #[test]
    fn encrypts_plaintext_rows_and_decrypts_back() {
        let key = [0x42u8; 32];
        let mut conn = setup();
        let (_, rr_id) = insert_plaintext_record(&conn, "Grocery Store", r#"{"memo":"groceries"}"#);

        encrypt_existing_fields(&mut conn, &key).unwrap();

        let (description, raw_json): (String, String) = conn
            .query_row(
                "SELECT description, raw_json FROM raw_records WHERE id = ?1",
                rusqlite::params![rr_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(
            description.starts_with("v1:"),
            "description must be encrypted"
        );
        assert!(raw_json.starts_with("v1:"), "raw_json must be encrypted");
        assert_eq!(open_field(&key, &description).unwrap(), "Grocery Store");
        assert_eq!(
            open_field(&key, &raw_json).unwrap(),
            r#"{"memo":"groceries"}"#
        );
    }

    #[test]
    fn idempotent_already_encrypted_rows_unchanged() {
        let key = [0x42u8; 32];
        let mut conn = setup();
        let (_, rr_id) = insert_plaintext_record(&conn, "Grocery Store", r#"{"memo":"groceries"}"#);

        encrypt_existing_fields(&mut conn, &key).unwrap();

        let (description_after_first, _): (String, String) = conn
            .query_row(
                "SELECT description, raw_json FROM raw_records WHERE id = ?1",
                rusqlite::params![rr_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();

        // Second run must not re-encrypt (different nonce would change the ciphertext).
        encrypt_existing_fields(&mut conn, &key).unwrap();

        let (description_after_second, _): (String, String) = conn
            .query_row(
                "SELECT description, raw_json FROM raw_records WHERE id = ?1",
                rusqlite::params![rr_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            description_after_first, description_after_second,
            "second run must not re-encrypt"
        );
    }

    #[test]
    fn null_notes_are_left_null() {
        let key = [0x42u8; 32];
        let mut conn = setup();
        let (tx_id, _) = insert_plaintext_record(&conn, "Coffee", "{}");
        conn.execute(
            "INSERT INTO transaction_meta (transaction_id, notes, tags, reviewed, updated_at)
             VALUES (?1, NULL, '[]', 0, datetime('now'))",
            rusqlite::params![tx_id],
        )
        .unwrap();

        encrypt_existing_fields(&mut conn, &key).unwrap();

        let notes: Option<String> = conn
            .query_row(
                "SELECT notes FROM transaction_meta WHERE transaction_id = ?1",
                rusqlite::params![tx_id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(notes.is_none(), "NULL notes must stay NULL after migration");
    }

    #[test]
    fn encrypts_transaction_meta_fields() {
        let key = [0x42u8; 32];
        let mut conn = setup();
        let (tx_id, _) = insert_plaintext_record(&conn, "Amazon", "{}");
        conn.execute(
            "INSERT INTO transaction_meta (transaction_id, notes, tags, reviewed, updated_at)
             VALUES (?1, 'work expense', '[\"work\"]', 0, datetime('now'))",
            rusqlite::params![tx_id],
        )
        .unwrap();

        encrypt_existing_fields(&mut conn, &key).unwrap();

        let (notes, tags): (String, String) = conn
            .query_row(
                "SELECT notes, tags FROM transaction_meta WHERE transaction_id = ?1",
                rusqlite::params![tx_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(notes.starts_with("v1:"), "notes must be encrypted");
        assert!(tags.starts_with("v1:"), "tags must be encrypted");
        assert_eq!(open_field(&key, &notes).unwrap(), "work expense");
        assert_eq!(open_field(&key, &tags).unwrap(), "[\"work\"]");
    }

    #[test]
    fn already_encrypted_meta_unchanged() {
        let key = [0x42u8; 32];
        let mut conn = setup();
        let (tx_id, _) = insert_plaintext_record(&conn, "Amazon", "{}");
        let sealed_notes = seal_field(&key, "work expense");
        let sealed_tags = seal_field(&key, "[\"work\"]");
        conn.execute(
            "INSERT INTO transaction_meta (transaction_id, notes, tags, reviewed, updated_at)
             VALUES (?1, ?2, ?3, 0, datetime('now'))",
            rusqlite::params![tx_id, sealed_notes.clone(), sealed_tags.clone()],
        )
        .unwrap();

        encrypt_existing_fields(&mut conn, &key).unwrap();

        let (notes_after, tags_after): (String, String) = conn
            .query_row(
                "SELECT notes, tags FROM transaction_meta WHERE transaction_id = ?1",
                rusqlite::params![tx_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            notes_after, sealed_notes,
            "already-encrypted notes unchanged"
        );
        assert_eq!(tags_after, sealed_tags, "already-encrypted tags unchanged");
    }
}
