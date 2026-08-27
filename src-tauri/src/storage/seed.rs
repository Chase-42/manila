use chrono::Utc;
use rusqlite::{Connection, Result};

// Stable UUIDs for groups; same idempotency guarantee as SEED_CATEGORIES.
const SEED_GROUPS: &[(&str, &str, i64)] = &[
    ("10000000-0000-4000-8000-000000000001", "Food & Dining", 1),
    ("10000000-0000-4000-8000-000000000002", "Transportation", 2),
    ("10000000-0000-4000-8000-000000000003", "Home", 3),
    ("10000000-0000-4000-8000-000000000004", "Health", 4),
    ("10000000-0000-4000-8000-000000000005", "Lifestyle", 5),
    ("10000000-0000-4000-8000-000000000006", "Savings", 6),
];

// Maps seeded category IDs to their default group IDs.
const SEED_CATEGORY_GROUPS: &[(&str, &str)] = &[
    // Food & Dining
    (
        "a1b2c3d4-e5f6-4a7b-8c9d-e0f1a2b3c4d5",
        "10000000-0000-4000-8000-000000000001",
    ), // Groceries
    (
        "b2c3d4e5-f6a7-4b8c-9d0e-f1a2b3c4d5e6",
        "10000000-0000-4000-8000-000000000001",
    ), // Dining Out
    // Transportation
    (
        "c3d4e5f6-a7b8-4c9d-0e1f-a2b3c4d5e6f7",
        "10000000-0000-4000-8000-000000000002",
    ), // Gas
    (
        "d4e5f6a7-b8c9-4d0e-1f2a-b3c4d5e6f7a8",
        "10000000-0000-4000-8000-000000000002",
    ), // Transportation
    (
        "c9d0e1f2-a3b4-4c5d-6e7f-a8b9c0d1e2f3",
        "10000000-0000-4000-8000-000000000002",
    ), // Car Maintenance
    // Home
    (
        "e5f6a7b8-c9d0-4e1f-2a3b-c4d5e6f7a8b9",
        "10000000-0000-4000-8000-000000000003",
    ), // Utilities
    (
        "d0e1f2a3-b4c5-4d6e-7f8a-b9c0d1e2f3a4",
        "10000000-0000-4000-8000-000000000003",
    ), // Home Maintenance
    // Health
    (
        "f6a7b8c9-d0e1-4f2a-3b4c-d5e6f7a8b9c0",
        "10000000-0000-4000-8000-000000000004",
    ), // Healthcare
    // Lifestyle
    (
        "a7b8c9d0-e1f2-4a3b-4c5d-e6f7a8b9c0d1",
        "10000000-0000-4000-8000-000000000005",
    ), // Subscriptions
    (
        "b8c9d0e1-f2a3-4b4c-5d6e-f7a8b9c0d1e2",
        "10000000-0000-4000-8000-000000000005",
    ), // Personal Care
    (
        "c5d6e7f8-a9b0-4c1d-2e3f-a4b5c6d7e8f9",
        "10000000-0000-4000-8000-000000000005",
    ), // Electronics
    (
        "d6e7f8a9-b0c1-4d2e-3f4a-b5c6d7e8f9a0",
        "10000000-0000-4000-8000-000000000005",
    ), // Clothing
    // Savings
    (
        "e1f2a3b4-c5d6-4e7f-8a9b-c0d1e2f3a4b5",
        "10000000-0000-4000-8000-000000000006",
    ), // Insurance
    (
        "f2a3b4c5-d6e7-4f8a-9b0c-d1e2f3a4b5c6",
        "10000000-0000-4000-8000-000000000006",
    ), // Travel
    (
        "a3b4c5d6-e7f8-4a9b-0c1d-e2f3a4b5c6d7",
        "10000000-0000-4000-8000-000000000006",
    ), // Gifts
    (
        "b4c5d6e7-f8a9-4b0c-1d2e-f3a4b5c6d7e8",
        "10000000-0000-4000-8000-000000000006",
    ), // Emergency Fund
];

// Stable UUIDs so INSERT OR IGNORE is genuinely idempotent across upgrades.
// Renaming a default category preserves its id; re-seeding won't overwrite the name.
const SEED_CATEGORIES: &[(&str, &str, &str)] = &[
    ("a1b2c3d4-e5f6-4a7b-8c9d-e0f1a2b3c4d5", "Groceries", "flow"),
    ("b2c3d4e5-f6a7-4b8c-9d0e-f1a2b3c4d5e6", "Dining Out", "flow"),
    ("c3d4e5f6-a7b8-4c9d-0e1f-a2b3c4d5e6f7", "Gas", "flow"),
    (
        "d4e5f6a7-b8c9-4d0e-1f2a-b3c4d5e6f7a8",
        "Transportation",
        "flow",
    ),
    ("e5f6a7b8-c9d0-4e1f-2a3b-c4d5e6f7a8b9", "Utilities", "flow"),
    ("f6a7b8c9-d0e1-4f2a-3b4c-d5e6f7a8b9c0", "Healthcare", "flow"),
    (
        "a7b8c9d0-e1f2-4a3b-4c5d-e6f7a8b9c0d1",
        "Subscriptions",
        "flow",
    ),
    (
        "b8c9d0e1-f2a3-4b4c-5d6e-f7a8b9c0d1e2",
        "Personal Care",
        "flow",
    ),
    (
        "c9d0e1f2-a3b4-4c5d-6e7f-a8b9c0d1e2f3",
        "Car Maintenance",
        "sinking",
    ),
    (
        "d0e1f2a3-b4c5-4d6e-7f8a-b9c0d1e2f3a4",
        "Home Maintenance",
        "sinking",
    ),
    (
        "e1f2a3b4-c5d6-4e7f-8a9b-c0d1e2f3a4b5",
        "Insurance",
        "sinking",
    ),
    ("f2a3b4c5-d6e7-4f8a-9b0c-d1e2f3a4b5c6", "Travel", "sinking"),
    ("a3b4c5d6-e7f8-4a9b-0c1d-e2f3a4b5c6d7", "Gifts", "sinking"),
    (
        "b4c5d6e7-f8a9-4b0c-1d2e-f3a4b5c6d7e8",
        "Emergency Fund",
        "sinking",
    ),
    (
        "c5d6e7f8-a9b0-4c1d-2e3f-a4b5c6d7e8f9",
        "Electronics",
        "sinking",
    ),
    (
        "d6e7f8a9-b0c1-4d2e-3f4a-b5c6d7e8f9a0",
        "Clothing",
        "sinking",
    ),
];

const SEED_INCOME_CATEGORIES: &[(&str, &str)] = &[
    ("20000000-0000-4000-8000-000000000001", "Paycheck"),
    ("20000000-0000-4000-8000-000000000002", "Freelance"),
    ("20000000-0000-4000-8000-000000000003", "Interest"),
    ("20000000-0000-4000-8000-000000000004", "Other Income"),
];

pub fn seed_income_categories(conn: &Connection) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    for (id, name) in SEED_INCOME_CATEGORIES {
        conn.execute(
            "INSERT OR IGNORE INTO income_categories (id, name, hidden, created_at) VALUES (?1, ?2, 0, ?3)",
            rusqlite::params![id, name, now],
        )?;
    }
    Ok(())
}

pub fn seed_categories(conn: &Connection) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    for (id, name, kind) in SEED_CATEGORIES {
        conn.execute(
            "INSERT OR IGNORE INTO categories (id, name, kind, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, name, kind, now],
        )?;
    }
    Ok(())
}

pub fn seed_category_groups(conn: &Connection) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    for (id, name, sort_order) in SEED_GROUPS {
        conn.execute(
            "INSERT OR IGNORE INTO category_groups (id, name, sort_order, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, name, sort_order, now],
        )?;
    }
    for (category_id, group_id) in SEED_CATEGORY_GROUPS {
        conn.execute(
            "UPDATE categories SET group_id = ?1 WHERE id = ?2",
            rusqlite::params![group_id, category_id],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{db::open_connection, migrations::run_migrations};

    fn seeded_conn() -> Connection {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();
        seed_categories(&conn).unwrap();
        seed_category_groups(&conn).unwrap();
        seed_income_categories(&conn).unwrap();
        conn
    }

    #[test]
    fn seed_income_creates_four_categories() {
        let conn = seeded_conn();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM income_categories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 4);
    }

    #[test]
    fn seed_income_is_idempotent() {
        let conn = seeded_conn();
        seed_income_categories(&conn).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM income_categories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 4);
    }

    #[test]
    fn seed_creates_sixteen_categories() {
        let conn = seeded_conn();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 16);
    }

    #[test]
    fn seed_is_idempotent() {
        let conn = seeded_conn();
        seed_categories(&conn).unwrap();
        seed_category_groups(&conn).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 16);
        let group_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM category_groups", [], |r| r.get(0))
            .unwrap();
        assert_eq!(group_count, 6);
    }

    #[test]
    fn seed_has_eight_flow_and_eight_sinking() {
        let conn = seeded_conn();
        let flow: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM categories WHERE kind = 'flow'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let sinking: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM categories WHERE kind = 'sinking'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(flow, 8);
        assert_eq!(sinking, 8);
    }

    #[test]
    fn seed_creates_six_groups() {
        let conn = seeded_conn();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM category_groups", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 6);
    }

    #[test]
    fn seed_assigns_all_sixteen_categories_to_groups() {
        let conn = seeded_conn();
        let unassigned: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM categories WHERE group_id IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            unassigned, 0,
            "all 16 seeded categories should have a group_id"
        );
    }

    #[test]
    fn seed_groups_are_ordered_by_sort_order() {
        let conn = seeded_conn();
        let names: Vec<String> = conn
            .prepare("SELECT name FROM category_groups ORDER BY sort_order")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(
            names,
            vec![
                "Food & Dining",
                "Transportation",
                "Home",
                "Health",
                "Lifestyle",
                "Savings"
            ]
        );
    }
}
