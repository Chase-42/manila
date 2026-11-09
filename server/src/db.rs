use rusqlite::{Connection, Result, params};
use std::sync::{Arc, Mutex};

pub const SUBSCRIPTION_GRACE_SECS: u64 = 7 * 24 * 60 * 60; // 7 days

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
        // Add subscription column to existing databases that predate this feature.
        // Ignore "duplicate column name" (error code 1) from SQLite versions < 3.35
        // that don't support IF NOT EXISTS on ALTER TABLE ADD COLUMN.
        let _ = conn.execute_batch(
            "ALTER TABLE devices ADD COLUMN subscription_expires_at TEXT;",
        );
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

    pub fn delete_device(&self, device_id: &str) -> Result<bool> {
        let conn = self.0.lock().unwrap();
        // Delete associated blobs before the device row to maintain FK integrity.
        // After revocation a re-enrolled device (new UUID, same keypair) starts
        // pushing new blobs; existing blobs from the old device are no longer
        // needed on the server because all devices retain the full dataset locally.
        conn.execute(
            "DELETE FROM sync_blobs WHERE device_id = ?1",
            params![device_id],
        )?;
        let rows_affected = conn.execute(
            "DELETE FROM devices WHERE id = ?1",
            params![device_id],
        )?;
        Ok(rows_affected > 0)
    }

    pub fn set_subscription(&self, device_id: &str, expires_at: &str) -> Result<bool> {
        let conn = self.0.lock().unwrap();
        let rows = conn.execute(
            "UPDATE devices SET subscription_expires_at = ?1 WHERE id = ?2",
            params![expires_at, device_id],
        )?;
        Ok(rows > 0)
    }

    pub fn find_device_subscription(&self, device_id: &str) -> Result<Option<String>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT subscription_expires_at FROM devices WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![device_id])?;
        if let Some(row) = rows.next()? {
            Ok(row.get(0)?)
        } else {
            Ok(None)
        }
    }

    // Returns true when the device has a subscription that has not expired
    // beyond the 7-day grace period (SUBSCRIPTION_GRACE_SECS).
    pub fn is_subscribed(&self, device_id: &str) -> Result<bool> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT subscription_expires_at FROM devices WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![device_id])?;
        let Some(row) = rows.next()? else {
            return Ok(false);
        };
        let expires_at: Option<String> = row.get(0)?;
        let Some(exp) = expires_at else {
            return Ok(false);
        };
        let exp_secs = parse_iso8601_secs(&exp).unwrap_or(0);
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Ok(now_secs <= exp_secs + SUBSCRIPTION_GRACE_SECS)
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

// Parses "YYYY-MM-DDTHH:MM:SSZ" to Unix seconds. Returns None on parse failure.
pub fn parse_iso8601_secs(s: &str) -> Option<u64> {
    let s = s.trim_end_matches('Z');
    let (date, time) = s.split_once('T')?;
    let mut dp = date.splitn(3, '-');
    let y: u64 = dp.next()?.parse().ok()?;
    let mo: u64 = dp.next()?.parse().ok()?;
    let d: u64 = dp.next()?.parse().ok()?;
    let mut tp = time.splitn(3, ':');
    let h: u64 = tp.next()?.parse().ok()?;
    let mi: u64 = tp.next()?.parse().ok()?;
    let sec: u64 = tp.next()?.parse().ok()?;

    // Days from epoch to start of year y.
    let mut days: u64 = 0;
    for yr in 1970..y {
        days += if is_leap_year(yr) { 366 } else { 365 };
    }
    // Days within the year up to month mo.
    let leap = is_leap_year(y);
    let month_days: [u64; 12] = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 1..mo {
        days += month_days.get((m - 1) as usize).copied()?;
    }
    days += d.checked_sub(1)?;

    Some(days * 86400 + h * 3600 + mi * 60 + sec)
}

pub(crate) fn is_leap_year(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
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

    #[test]
    fn delete_device_removes_row_and_returns_true() {
        let db = Db::open_in_memory().unwrap();
        db.get_or_create_device("dev-1", "aabbcc", "2024-01-01T00:00:00Z").unwrap();
        assert!(db.delete_device("dev-1").unwrap());
        assert!(db.find_device_by_id("dev-1").unwrap().is_none());
    }

    #[test]
    fn delete_device_missing_returns_false() {
        let db = Db::open_in_memory().unwrap();
        assert!(!db.delete_device("nonexistent").unwrap());
    }

    #[test]
    fn delete_device_also_removes_associated_blobs() {
        let db = Db::open_in_memory().unwrap();
        db.get_or_create_device("dev-1", "aabbcc", "2024-01-01T00:00:00Z").unwrap();
        db.insert_blob("dev-1", b"payload1", "2024-01-01T00:00:00Z").unwrap();
        db.insert_blob("dev-1", b"payload2", "2024-01-01T00:00:01Z").unwrap();
        assert!(db.delete_device("dev-1").unwrap());
        let blobs = db.pull_blobs(0).unwrap();
        assert_eq!(blobs.len(), 0);
    }

    // ── Subscription tests ────────────────────────────────────────────────────

    #[test]
    fn unsubscribed_device_is_not_subscribed() {
        let db = Db::open_in_memory().unwrap();
        db.get_or_create_device("dev-1", "aabbcc", "2024-01-01T00:00:00Z").unwrap();
        assert!(!db.is_subscribed("dev-1").unwrap());
    }

    #[test]
    fn unknown_device_is_not_subscribed() {
        let db = Db::open_in_memory().unwrap();
        assert!(!db.is_subscribed("nonexistent").unwrap());
    }

    #[test]
    fn set_subscription_then_is_subscribed() {
        let db = Db::open_in_memory().unwrap();
        db.get_or_create_device("dev-1", "aabbcc", "2024-01-01T00:00:00Z").unwrap();
        // expires far in the future
        assert!(db.set_subscription("dev-1", "2099-01-01T00:00:00Z").unwrap());
        assert!(db.is_subscribed("dev-1").unwrap());
    }

    #[test]
    fn set_subscription_missing_device_returns_false() {
        let db = Db::open_in_memory().unwrap();
        assert!(!db.set_subscription("ghost", "2099-01-01T00:00:00Z").unwrap());
    }

    #[test]
    fn find_device_subscription_returns_value_after_set() {
        let db = Db::open_in_memory().unwrap();
        db.get_or_create_device("dev-1", "aabbcc", "2024-01-01T00:00:00Z").unwrap();
        db.set_subscription("dev-1", "2099-06-15T12:00:00Z").unwrap();
        assert_eq!(
            db.find_device_subscription("dev-1").unwrap().as_deref(),
            Some("2099-06-15T12:00:00Z"),
        );
    }

    #[test]
    fn find_device_subscription_returns_none_for_unknown() {
        let db = Db::open_in_memory().unwrap();
        // Device doesn't exist - returns Ok(None), not an error.
        assert!(db.find_device_subscription("ghost").unwrap().is_none());
    }

    #[test]
    fn grace_period_within_7_days_still_subscribed() {
        let db = Db::open_in_memory().unwrap();
        db.get_or_create_device("dev-1", "aabbcc", "2024-01-01T00:00:00Z").unwrap();
        // Set expiry to 6 days ago (within grace window).
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let exp_secs = now_secs - 6 * 24 * 3600;
        let exp_str = secs_to_iso8601(exp_secs);
        db.set_subscription("dev-1", &exp_str).unwrap();
        assert!(db.is_subscribed("dev-1").unwrap());
    }

    #[test]
    fn grace_period_beyond_7_days_not_subscribed() {
        let db = Db::open_in_memory().unwrap();
        db.get_or_create_device("dev-1", "aabbcc", "2024-01-01T00:00:00Z").unwrap();
        // Set expiry to 8 days ago (outside grace window).
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let exp_secs = now_secs - 8 * 24 * 3600;
        let exp_str = secs_to_iso8601(exp_secs);
        db.set_subscription("dev-1", &exp_str).unwrap();
        assert!(!db.is_subscribed("dev-1").unwrap());
    }

    #[test]
    fn parse_iso8601_secs_known_epoch() {
        // 1970-01-01T00:00:00Z should be 0
        assert_eq!(parse_iso8601_secs("1970-01-01T00:00:00Z"), Some(0));
    }

    #[test]
    fn parse_iso8601_secs_known_value() {
        // 2024-01-01T00:00:00Z = 1704067200
        assert_eq!(parse_iso8601_secs("2024-01-01T00:00:00Z"), Some(1704067200));
    }

    // Helper: convert Unix seconds back to an ISO-8601 string for test setup.
    fn secs_to_iso8601(secs: u64) -> String {
        let s = secs % 60;
        let total_minutes = secs / 60;
        let mi = total_minutes % 60;
        let total_hours = total_minutes / 60;
        let h = total_hours % 24;
        let mut days = total_hours / 24;
        let mut y = 1970u64;
        loop {
            let dy = if is_leap_year(y) { 366 } else { 365 };
            if days < dy { break; }
            days -= dy;
            y += 1;
        }
        let leap = is_leap_year(y);
        let months = [31u64, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        let mut mo = 1u64;
        for dm in &months {
            if days < *dm { break; }
            days -= dm;
            mo += 1;
        }
        format!("{y:04}-{mo:02}-{:02}T{h:02}:{mi:02}:{s:02}Z", days + 1)
    }
}
