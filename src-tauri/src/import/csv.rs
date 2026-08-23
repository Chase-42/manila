use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct CsvPreview {
    pub headers: Vec<String>,
    pub sample_rows: Vec<Vec<String>>,
}

/// Column mapping provided by the user after previewing the CSV headers.
///
/// Either `amount_col` must be Some (single-column mode) or both
/// `debit_col` and `credit_col` must be Some (split-column mode).
#[derive(Debug, Deserialize)]
pub struct ColumnMapping {
    pub date_col: String,
    pub description_col: String,
    pub amount_col: Option<String>,
    /// Negate the value from `amount_col` before storing. Used when the bank
    /// exports debits as positive numbers.
    pub flip_sign: bool,
    pub debit_col: Option<String>,
    pub credit_col: Option<String>,
}

/// One successfully parsed row, ready for DB insertion.
#[derive(Debug)]
pub struct ParsedRow {
    pub date: String,
    pub amount_cents: i64,
    pub description: String,
    /// Original row serialized as a JSON object keyed by header name.
    pub raw_json: String,
}

/// Strip currency symbols, commas, and parenthetical negation; return cents.
///
/// Handles: "$1,234.56", "(50.00)", "-12.50", "12.50", "1234", "0.01".
/// Sign convention: negative = outflow, positive = inflow.
pub fn parse_amount(s: &str) -> Result<i64, String> {
    let s = s.trim();
    let negative = s.starts_with('(') && s.ends_with(')');
    // Strip parentheses, leading sign, and currency symbols.
    let stripped: String = s
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    let value: f64 = stripped
        .parse()
        .map_err(|_| format!("cannot parse amount: {s:?}"))?;
    let cents = (value.abs() * 100.0).round() as i64;
    if negative || value < 0.0 {
        Ok(-cents)
    } else {
        Ok(cents)
    }
}

// DATE_FORMATS tried in order; first match wins.
const DATE_FORMATS: &[&str] = &[
    "%Y-%m-%d", // ISO: 2026-08-22
    "%m/%d/%Y", // US full: 08/22/2026 or 8/22/2026
    "%m/%d/%y", // US short: 08/22/26
];

/// Parse a date string into YYYY-MM-DD. Tries ISO then common US formats.
///
/// 2-digit years (via %y) are adjusted to 2000+: chrono returns them as
/// 00xx, which would silently misfiled transactions.
pub fn parse_date(s: &str) -> Result<String, String> {
    let s = s.trim();
    for fmt in DATE_FORMATS {
        if let Ok(d) = NaiveDate::parse_from_str(s, fmt) {
            let d = if d.year() < 100 {
                d.with_year(d.year() + 2000)
                    .ok_or_else(|| format!("year adjustment failed for {s:?}"))?
            } else {
                d
            };
            return Ok(d.format("%Y-%m-%d").to_string());
        }
    }
    Err(format!("unrecognized date format: {s:?}"))
}

/// Read the CSV header row and up to 5 data rows for the column-mapping UI.
pub fn extract_preview(content: &str) -> Result<CsvPreview, String> {
    let mut rdr = csv::Reader::from_reader(content.as_bytes());
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| format!("CSV read error: {e}"))?
        .iter()
        .map(|h| h.to_owned())
        .collect();

    let sample_rows: Vec<Vec<String>> = rdr
        .records()
        .take(5)
        .map(|r| {
            r.map(|rec| rec.iter().map(|f| f.to_owned()).collect())
                .map_err(|e| format!("CSV read error: {e}"))
        })
        .collect::<Result<_, _>>()?;

    Ok(CsvPreview { headers, sample_rows })
}

fn col_index(headers: &[String], col: &str) -> Result<usize, String> {
    headers
        .iter()
        .position(|h| h == col)
        .ok_or_else(|| format!("column {col:?} not found in CSV headers"))
}

/// Parse every data row using the provided column mapping.
///
/// Returns one Result per row; parse errors are Err so the caller can count
/// skipped rows and collect their messages without aborting the import.
pub fn parse_rows(
    content: &str,
    mapping: &ColumnMapping,
) -> Result<Vec<Result<ParsedRow, String>>, String> {
    let mut rdr = csv::Reader::from_reader(content.as_bytes());
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| format!("CSV read error: {e}"))?
        .iter()
        .map(|h| h.to_owned())
        .collect();

    let date_idx = col_index(&headers, &mapping.date_col)?;
    let desc_idx = col_index(&headers, &mapping.description_col)?;

    // Validate amount columns are present before iterating rows.
    let amount_mode = if let Some(col) = &mapping.amount_col {
        AmountMode::Single { idx: col_index(&headers, col)? }
    } else {
        let d = mapping.debit_col.as_deref().ok_or("debit_col required in debit/credit mode")?;
        let c = mapping.credit_col.as_deref().ok_or("credit_col required in debit/credit mode")?;
        AmountMode::Split { debit_idx: col_index(&headers, d)?, credit_idx: col_index(&headers, c)? }
    };

    let mut results = Vec::new();
    for record in rdr.records() {
        let record = match record {
            Ok(r) => r,
            Err(e) => {
                results.push(Err(format!("CSV read error: {e}")));
                continue;
            }
        };

        let row: Vec<String> = record.iter().map(|f| f.to_owned()).collect();

        // Build raw_json from header->value pairs.
        let raw_map: serde_json::Map<String, serde_json::Value> = headers
            .iter()
            .zip(row.iter())
            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
            .collect();
        let raw_json = serde_json::to_string(&raw_map).unwrap_or_else(|_| "{}".into());

        let parsed = parse_row(&row, date_idx, desc_idx, &amount_mode, mapping.flip_sign, &raw_json);
        results.push(parsed);
    }

    Ok(results)
}

enum AmountMode {
    Single { idx: usize },
    Split { debit_idx: usize, credit_idx: usize },
}

fn parse_row(
    row: &[String],
    date_idx: usize,
    desc_idx: usize,
    amount_mode: &AmountMode,
    flip_sign: bool,
    raw_json: &str,
) -> Result<ParsedRow, String> {
    let date_str = row.get(date_idx).map(|s| s.as_str()).unwrap_or("");
    let desc = row.get(desc_idx).map(|s| s.trim().to_owned()).unwrap_or_default();

    if date_str.trim().is_empty() {
        return Err("empty date cell".into());
    }
    let date = parse_date(date_str)?;

    let amount_cents = match amount_mode {
        AmountMode::Single { idx } => {
            let cell = row.get(*idx).map(|s| s.as_str()).unwrap_or("");
            if cell.trim().is_empty() {
                return Err("empty amount cell".into());
            }
            let cents = parse_amount(cell)?;
            if flip_sign { -cents } else { cents }
        }
        AmountMode::Split { debit_idx, credit_idx } => {
            let debit_cell = row.get(*debit_idx).map(|s| s.trim()).unwrap_or("");
            let credit_cell = row.get(*credit_idx).map(|s| s.trim()).unwrap_or("");
            // Exactly one side should be non-empty per row; if both or neither, error.
            match (debit_cell.is_empty(), credit_cell.is_empty()) {
                (true, true) => return Err("both debit and credit cells are empty".into()),
                (false, false) => {
                    // Some banks fill both columns; use debit if non-zero else credit.
                    let d = parse_amount(debit_cell)?;
                    let c = parse_amount(credit_cell)?;
                    if d != 0 { -d } else { c }
                }
                (false, true) => -parse_amount(debit_cell)?,  // debit = outflow
                (true, false) => parse_amount(credit_cell)?,  // credit = inflow
            }
        }
    };

    Ok(ParsedRow { date, amount_cents, description: desc, raw_json: raw_json.to_owned() })
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_amount ---

    #[test]
    fn amount_dollar_with_commas() {
        assert_eq!(parse_amount("$1,234.56").unwrap(), 123456);
    }

    #[test]
    fn amount_parenthetical_negative() {
        assert_eq!(parse_amount("(50.00)").unwrap(), -5000);
    }

    #[test]
    fn amount_explicit_negative() {
        assert_eq!(parse_amount("-12.50").unwrap(), -1250);
    }

    #[test]
    fn amount_plain_positive() {
        assert_eq!(parse_amount("12.50").unwrap(), 1250);
    }

    #[test]
    fn amount_integer() {
        assert_eq!(parse_amount("1234").unwrap(), 123400);
    }

    #[test]
    fn amount_one_cent() {
        assert_eq!(parse_amount("0.01").unwrap(), 1);
    }

    // --- parse_date ---

    #[test]
    fn date_iso() {
        assert_eq!(parse_date("2026-08-22").unwrap(), "2026-08-22");
    }

    #[test]
    fn date_us_padded() {
        assert_eq!(parse_date("08/22/2026").unwrap(), "2026-08-22");
    }

    #[test]
    fn date_us_unpadded() {
        assert_eq!(parse_date("8/22/2026").unwrap(), "2026-08-22");
    }

    #[test]
    fn date_us_short_year() {
        assert_eq!(parse_date("08/22/26").unwrap(), "2026-08-22");
    }
}
