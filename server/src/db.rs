use rusqlite::{Connection, Result, params};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Db(pub Arc<Mutex<Connection>>);

impl Db {
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self(Arc::new(Mutex::new(conn)));
        db.init()?;
        Ok(db)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self(Arc::new(Mutex::new(conn)));
        db.init()?;
        Ok(db)
    }

    fn init(&self) -> Result<()> {
        let conn = self.0.lock().unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS devices (
                id              TEXT PRIMARY KEY,
                public_key_hex  TEXT UNIQUE NOT NULL,
                enrolled_at     TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS sync_blobs (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                device_id   TEXT NOT NULL REFERENCES devices(id),
                payload     BLOB NOT NULL,
                created_at  TEXT NOT NULL
            );",
        )?;
        Ok(())
    }

    // Atomically insert-or-ignore then select, so concurrent enrollments with the
    // same key always return the same device_id without a TOCTOU race.
    pub fn get_or_create_device(
        &self,
        candidate_id: &str,
        public_key_hex: &str,
        enrolled_at: &str,
    ) -> Result<(String, String)> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO devices (id, public_key_hex, enrolled_at) VALUES (?1, ?2, ?3)",
            params![candidate_id, public_key_hex, enrolled_at],
        )?;
        let mut stmt =
            conn.prepare("SELECT id, enrolled_at FROM devices WHERE public_key_hex = ?1")?;
        let mut rows = stmt.query(params![public_key_hex])?;
        rows.next()?
            .map(|row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap_or(Err(rusqlite::Error::QueryReturnedNoRows))
    }

    pub fn find_device_by_id(&self, id: &str) -> Result<Option<String>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare("SELECT public_key_hex FROM devices WHERE id = ?1")?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn insert_blob(&self, device_id: &str, payload: &[u8], created_at: &str) -> Result<i64> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "INSERT INTO sync_blobs (device_id, payload, created_at) VALUES (?1, ?2, ?3)",
            params![device_id, payload, created_at],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn pull_blobs(&self, after: i64) -> Result<Vec<BlobRow>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, device_id, payload, created_at FROM sync_blobs WHERE id > ?1 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![after], |row| {
            Ok(BlobRow {
                seq: row.get(0)?,
                device_id: row.get(1)?,
                payload: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;
        rows.collect()
    }
}

pub struct BlobRow {
    pub seq: i64,
    pub device_id: String,
    pub payload: Vec<u8>,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_creates_both_tables() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.0.lock().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('devices', 'sync_blobs')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn enroll_and_find_device() {
        let db = Db::open_in_memory().unwrap();
        let (id, _) = db.get_or_create_device("dev-1", "aabbcc", "2024-01-01T00:00:00Z").unwrap();
        assert_eq!(id, "dev-1");
    }

    #[test]
    fn enroll_idempotent() {
        let db = Db::open_in_memory().unwrap();
        let (id1, _) = db.get_or_create_device("dev-1", "aabbcc", "2024-01-01T00:00:00Z").unwrap();
        let (id2, _) = db.get_or_create_device("dev-2", "aabbcc", "2024-01-02T00:00:00Z").unwrap();
        assert_eq!(id1, id2);
    }

    #[test]
    fn insert_and_pull_blobs() {
        let db = Db::open_in_memory().unwrap();
        db.get_or_create_device("dev-1", "aabbcc", "2024-01-01T00:00:00Z").unwrap();
        let seq1 = db.insert_blob("dev-1", b"payload1", "2024-01-01T00:00:00Z").unwrap();
        let seq2 = db.insert_blob("dev-1", b"payload2", "2024-01-01T00:00:01Z").unwrap();
        let blobs = db.pull_blobs(0).unwrap();
        assert_eq!(blobs.len(), 2);
        assert_eq!(blobs[0].seq, seq1);
        assert_eq!(blobs[1].seq, seq2);
        let blobs_after = db.pull_blobs(seq1).unwrap();
        assert_eq!(blobs_after.len(), 1);
        assert_eq!(blobs_after[0].seq, seq2);
    }
}
