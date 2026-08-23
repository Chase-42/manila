use serde_json;

use super::ParsedRow;
use super::csv::parse_amount;

/// Extract the text value of the first occurrence of `tag` within `block`.
///
/// Works for both OFX 1.x (leaf elements have no closing tag; value ends at the
/// next `<`) and OFX 2.x (value ends at `</TAG>`). Tag matching is
/// case-insensitive because exporters vary.
fn extract_tag(block: &str, tag: &str) -> Option<String> {
    let lower_block = block.to_lowercase();
    let open_tag = format!("<{}>", tag.to_lowercase());
    let pos = lower_block.find(&open_tag)?;
    let start = pos + open_tag.len();
    let rest = &block[start..];
    let end = rest.find('<').unwrap_or(rest.len());
    let value = rest[..end].trim().to_owned();
    if value.is_empty() { None } else { Some(value) }
}

/// Take the first 8 chars of an OFX date string (YYYYMMDD) and return YYYY-MM-DD.
///
/// OFX dates may include time and timezone (`20260115120000.000[-5:EST]`); we
/// intentionally strip everything past the date because transaction dates are
/// calendar dates with no time and no timezone per the coding standards.
fn parse_ofx_date(s: &str) -> Result<String, String> {
    let s = s.trim();
    if s.len() < 8 {
        return Err(format!("OFX date too short: {s:?}"));
    }
    let digits = &s[..8];
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("OFX date has non-digit chars: {s:?}"));
    }
    Ok(format!("{}-{}-{}", &digits[..4], &digits[4..6], &digits[6..8]))
}

fn parse_stmttrn(block: &str, account_id: &str) -> Result<ParsedRow, String> {
    let dtposted = extract_tag(block, "DTPOSTED")
        .ok_or_else(|| "missing DTPOSTED".to_owned())?;
    let date = parse_ofx_date(&dtposted)?;

    let trnamt = extract_tag(block, "TRNAMT")
        .ok_or_else(|| "missing TRNAMT".to_owned())?;
    let amount_cents = parse_amount(&trnamt)?;

    let name = extract_tag(block, "NAME").unwrap_or_default();
    let memo = extract_tag(block, "MEMO").unwrap_or_default();
    let description = if name.is_empty() && memo.is_empty() {
        return Err("missing NAME and MEMO".to_owned());
    } else if name.is_empty() {
        memo.clone()
    } else if !memo.is_empty() && memo != name {
        format!("{name} - {memo}")
    } else {
        name.clone()
    };

    let fitid = extract_tag(block, "FITID");
    let source_id = Some(match &fitid {
        Some(id) => format!("ofx|{account_id}|{id}"),
        None => format!("ofx|{account_id}|{date}|{amount_cents}|{description}"),
    });

    let mut raw_map = serde_json::Map::new();
    raw_map.insert("DTPOSTED".into(), serde_json::Value::String(dtposted));
    raw_map.insert("TRNAMT".into(), serde_json::Value::String(trnamt));
    if let Some(id) = fitid {
        raw_map.insert("FITID".into(), serde_json::Value::String(id));
    }
    if !name.is_empty() {
        raw_map.insert("NAME".into(), serde_json::Value::String(name));
    }
    if !memo.is_empty() {
        raw_map.insert("MEMO".into(), serde_json::Value::String(memo));
    }
    let raw_json = serde_json::to_string(&raw_map).unwrap_or_else(|_| "{}".into());

    Ok(ParsedRow { date, amount_cents, description, raw_json, source_id })
}

/// Parse an OFX or QFX file (1.x SGML or 2.x XML) and return one result per
/// `<STMTTRN>` block. The outer Result is for file-level failures; per-row
/// errors are inner Err values so the caller can count skipped rows.
pub fn parse_ofx(content: &str, account_id: &str) -> Result<Vec<Result<ParsedRow, String>>, String> {
    let lower = content.to_lowercase();
    let open_tag = "<stmttrn>";
    let close_tag = "</stmttrn>";

    let mut results = Vec::new();
    let mut search_start = 0;

    while let Some(rel_close) = lower[search_start..].find(close_tag) {
        let close_pos = search_start + rel_close;
        let segment = &content[search_start..close_pos];
        let lower_segment = &lower[search_start..close_pos];

        if let Some(open_pos) = lower_segment.rfind(open_tag) {
            let block = &segment[open_pos + open_tag.len()..];
            results.push(parse_stmttrn(block, account_id));
        }

        search_start = close_pos + close_tag.len();
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_ofx_date ---

    #[test]
    fn date_bare_eight_chars() {
        assert_eq!(parse_ofx_date("20260115").unwrap(), "2026-01-15");
    }

    #[test]
    fn date_with_time() {
        assert_eq!(parse_ofx_date("20260115120000").unwrap(), "2026-01-15");
    }

    #[test]
    fn date_with_tz_suffix() {
        assert_eq!(parse_ofx_date("20260120000000.000[-5:EST]").unwrap(), "2026-01-20");
    }

    #[test]
    fn date_too_short_errors() {
        assert!(parse_ofx_date("2026").is_err());
    }

    // --- OFX 1.x (SGML-style, uppercase tags, no closing leaf tags) ---

    const OFX1_TWO_TXN: &str = "OFXHEADER:100\nDATA:OFXSGML\n\n\
        <OFX><BANKMSGSRSV1><STMTTRNRS><STMTRS><BANKTRANLIST>\
        <STMTTRN>\
        <TRNTYPE>DEBIT\
        <DTPOSTED>20260115120000\
        <TRNAMT>-45.67\
        <FITID>2026011501\
        <NAME>Grocery Store\
        </STMTTRN>\
        <STMTTRN>\
        <TRNTYPE>CREDIT\
        <DTPOSTED>20260120000000.000[-5:EST]\
        <TRNAMT>2000.00\
        <FITID>2026012001\
        <NAME>Paycheck\
        <MEMO>Direct Deposit\
        </STMTTRN>\
        </BANKTRANLIST></STMTRS></STMTTRNRS></BANKMSGSRSV1></OFX>";

    #[test]
    fn ofx1_parses_two_transactions() {
        let rows = parse_ofx(OFX1_TWO_TXN, "acct-1").unwrap();
        assert_eq!(rows.len(), 2);

        let r0 = rows[0].as_ref().unwrap();
        assert_eq!(r0.date, "2026-01-15");
        assert_eq!(r0.amount_cents, -4567);
        assert_eq!(r0.description, "Grocery Store");
        assert_eq!(r0.source_id.as_deref(), Some("ofx|acct-1|2026011501"));

        let r1 = rows[1].as_ref().unwrap();
        assert_eq!(r1.date, "2026-01-20");
        assert_eq!(r1.amount_cents, 200000);
        assert_eq!(r1.description, "Paycheck - Direct Deposit");
        assert_eq!(r1.source_id.as_deref(), Some("ofx|acct-1|2026012001"));
    }

    // --- OFX 2.x (XML, proper closing tags) ---

    const OFX2_TWO_TXN: &str = "\
        <?xml version=\"1.0\" encoding=\"UTF-8\"?>\
        <?OFX OFXHEADER=\"200\"?>\
        <OFX><BANKMSGSRSV1><STMTTRNRS><STMTRS><BANKTRANLIST>\
        <STMTTRN>\
        <TRNTYPE>DEBIT</TRNTYPE>\
        <DTPOSTED>20260101120000.000[-5:EST]</DTPOSTED>\
        <TRNAMT>-32.10</TRNAMT>\
        <FITID>2026010101</FITID>\
        <NAME>Gas Station</NAME>\
        </STMTTRN>\
        <STMTTRN>\
        <TRNTYPE>CREDIT</TRNTYPE>\
        <DTPOSTED>20260115000000</DTPOSTED>\
        <TRNAMT>1500.00</TRNAMT>\
        <FITID>2026011501</FITID>\
        <NAME>Employer</NAME>\
        <MEMO>Salary payment</MEMO>\
        </STMTTRN>\
        </BANKTRANLIST></STMTRS></STMTTRNRS></BANKMSGSRSV1></OFX>";

    #[test]
    fn ofx2_parses_two_transactions() {
        let rows = parse_ofx(OFX2_TWO_TXN, "acct-2").unwrap();
        assert_eq!(rows.len(), 2);

        let r0 = rows[0].as_ref().unwrap();
        assert_eq!(r0.date, "2026-01-01");
        assert_eq!(r0.amount_cents, -3210);
        assert_eq!(r0.description, "Gas Station");
        assert_eq!(r0.source_id.as_deref(), Some("ofx|acct-2|2026010101"));

        let r1 = rows[1].as_ref().unwrap();
        assert_eq!(r1.date, "2026-01-15");
        assert_eq!(r1.amount_cents, 150000);
        assert_eq!(r1.description, "Employer - Salary payment");
        assert_eq!(r1.source_id.as_deref(), Some("ofx|acct-2|2026011501"));
    }

    // --- FITID fallback ---

    #[test]
    fn missing_fitid_uses_date_amount_desc_fallback() {
        let content = "<OFX><BANKTRANLIST>\
            <STMTTRN>\
            <DTPOSTED>20260310\
            <TRNAMT>-10.00\
            <NAME>Coffee Shop\
            </STMTTRN>\
            </BANKTRANLIST></OFX>";
        let rows = parse_ofx(content, "acct-3").unwrap();
        assert_eq!(rows.len(), 1);
        let r = rows[0].as_ref().unwrap();
        assert_eq!(
            r.source_id.as_deref(),
            Some("ofx|acct-3|2026-03-10|-1000|Coffee Shop")
        );
    }

    // --- Empty file ---

    #[test]
    fn no_stmttrn_blocks_returns_empty_vec() {
        let rows = parse_ofx("OFXHEADER:100\nDATA:OFXSGML\n<OFX></OFX>", "acct-4").unwrap();
        assert!(rows.is_empty());
    }

    // --- Memo appended only when different from name ---

    #[test]
    fn memo_same_as_name_not_appended() {
        let content = "<OFX><BANKTRANLIST>\
            <STMTTRN>\
            <DTPOSTED>20260310\
            <TRNAMT>-5.00\
            <FITID>abc\
            <NAME>Coffee\
            <MEMO>Coffee\
            </STMTTRN>\
            </BANKTRANLIST></OFX>";
        let rows = parse_ofx(content, "a").unwrap();
        assert_eq!(rows[0].as_ref().unwrap().description, "Coffee");
    }
}
