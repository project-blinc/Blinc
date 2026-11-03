//! The conformance ledger: what `blinc_css` says of each declaration, against what
//! `blinc_abi::css` says of it, recorded in `conformance/values.tsv`.
//!
//! A row is a verdict from each engine. When this test fails the ledger is stale: a gap closed
//! (update the row) or opened (fix it, or record it with the issue that tracks it). The same file
//! is checked against the other engine in blinc_abi.

use blinc_css::{Severity, Stylesheet};

const LEDGER: &str = include_str!("conformance/values.tsv");

/// ok, reject, or unrouted for a property `blinc_css` does not know.
fn verdict(name: &str, value: &str) -> &'static str {
    let unknown = |e: &blinc_css::ParseError| e.message.to_lowercase().contains("unknown property");
    let known = Stylesheet::parse_with_errors(&format!(".x {{ {name}: 0; }}"))
        .errors
        .iter()
        .all(|e| !unknown(e));
    if !known {
        return "unrouted";
    }
    let r = Stylesheet::parse_with_errors(&format!(".x {{ {name}: {value}; }}"));
    let bad = r.errors.iter().any(|e| {
        matches!(e.severity, Severity::Error) || e.message.to_lowercase().contains("invalid")
    });
    if bad { "reject" } else { "ok" }
}

#[test]
fn blinc_css_matches_its_column_of_the_ledger() {
    blinc_theme::ThemeState::init_default();
    let mut stale = Vec::new();
    let mut rows = 0;
    for line in LEDGER
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let cols: Vec<&str> = line.split('\t').collect();
        assert!(cols.len() >= 4, "bad ledger row: {line}");
        rows += 1;
        let got = verdict(cols[0], cols[1]);
        if got != cols[2] {
            stale.push(format!(
                "{}: {} is {got}, the ledger says {}",
                cols[0], cols[1], cols[2]
            ));
        }
    }
    assert!(rows > 100, "the ledger is nearly empty: {rows} rows");
    assert!(stale.is_empty(), "stale ledger rows:\n{}", stale.join("\n"));
}

/// Every row where the engines differ says why, and every row that says why does differ.
#[test]
fn only_divergent_rows_carry_a_note() {
    for line in LEDGER
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let cols: Vec<&str> = line.split('\t').collect();
        let differs = cols[2] != cols[3];
        let noted = cols.get(4).is_some_and(|n| !n.is_empty());
        assert_eq!(differs, noted, "{line}");
    }
}
