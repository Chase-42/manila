// Domain types for each ledger table.
// amounts: i64 cents (never f64). dates: YYYY-MM-DD strings. ids: UUID strings.
// The "type" column on accounts maps to account_type here; "type" is a Rust keyword.

#[derive(Debug, Clone)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub account_type: String,
    pub subtype: String,
    pub institution: String,
    pub currency: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct Transaction {
    pub id: String,
    pub account_id: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct ImportBatch {
    pub id: String,
    pub source_type: String,
    pub account_id: Option<String>,
    pub filename: Option<String>,
    pub imported_at: String,
}

#[derive(Debug, Clone)]
pub struct RawRecord {
    pub id: String,
    pub transaction_id: String,
    pub supersedes_id: Option<String>,
    pub import_batch_id: Option<String>,
    pub source_id: String,
    pub date: String,
    pub amount_cents: i64,
    pub description: String,
    pub raw_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct Posting {
    pub id: String,
    pub transaction_id: String,
    pub account_id: String,
    pub amount_cents: i64,
}
