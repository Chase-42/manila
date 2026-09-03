pub mod csv;
pub mod dedup;
pub mod ofx;

use crate::types::Cents;

#[derive(Debug)]
pub struct ParsedRow {
    pub date: String,
    pub amount_cents: Cents,
    pub description: String,
    pub raw_json: String,
    /// Set by the parser when it can derive a stable dedup key (e.g. OFX FITID).
    /// None means the command layer generates the source_id from date/amount/desc.
    pub source_id: Option<String>,
}
