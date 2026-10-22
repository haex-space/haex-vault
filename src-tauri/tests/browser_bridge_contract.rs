//! Spec 049, FR-041, T023: the method table of the External Bridge is the contract. Every method of
//! `browser_bridge::methods::METHODS` has its section in `contracts/wire-protocol.md` §Methoden,
//! the unsupported list matches the contract's, and no handler lives under `chat`, `llm`,
//! `adapters` or `providers`: a browser extension never reaches the model side of holzi (ADR-0004).

use std::collections::BTreeSet;
use std::path::Path;

use holzi_lib::browser_bridge::methods::{METHODS, NOT_SUPPORTED};

fn contract() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../specs/049-external-bridge/contracts/wire-protocol.md");
    std::fs::read_to_string(path).expect("wire protocol contract")
}

fn section<'a>(text: &'a str, heading: &str) -> &'a str {
    let start = text.find(heading).expect(heading);
    let rest = &text[start..];
    let level = heading.split(' ').next().unwrap_or("##");
    let end = rest[heading.len()..]
        .find(&format!("\n{level} "))
        .map_or(rest.len(), |i| i + heading.len());
    &rest[..end]
}

/// The method names of the contract: the headings `### \`name\`` under §Methoden.
fn contract_methods(text: &str) -> BTreeSet<String> {
    section(text, "## Methoden")
        .lines()
        .filter_map(|line| line.strip_prefix("### `"))
        .filter_map(|rest| rest.split('`').next())
        .map(str::to_owned)
        .collect()
}

/// The names in backticks of the paragraph §Nicht unterstützt.
fn contract_unsupported(text: &str) -> BTreeSet<String> {
    let paragraph = section(text, "### Nicht unterstützt");
    let body = paragraph.split_once("\n\n").map_or("", |(_, body)| body);
    let body = body.split("\n\n").next().unwrap_or("");
    body.split('`')
        .skip(1)
        .step_by(2)
        .filter(|name| name.contains('-') && !name.chars().any(char::is_uppercase))
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_table_is_exactly_the_methods_of_the_contract() {
    let listed = contract_methods(&contract());
    let table: BTreeSet<String> = METHODS.iter().map(|m| m.name.to_owned()).collect();
    assert_eq!(table, listed, "METHODS against contracts/wire-protocol.md");
    assert_eq!(table.len(), 8);
}

#[test]
fn the_unsupported_methods_are_the_contract_list() {
    let text = contract();
    let expected = contract_unsupported(&text);
    let actual: BTreeSet<String> = NOT_SUPPORTED.iter().map(|n| (*n).to_owned()).collect();
    assert_eq!(actual, expected);
    let methods = contract_methods(&text);
    assert!(
        actual.is_disjoint(&methods),
        "a method is both served and unsupported"
    );
}

#[test]
fn no_method_reaches_the_model_side() {
    for method in METHODS {
        for forbidden in ["::chat", "::llm", "::adapters", "::providers"] {
            assert!(
                !method.module.contains(forbidden),
                "{} lives in {}",
                method.name,
                method.module
            );
        }
        assert!(
            method.module.starts_with("holzi_lib::browser_bridge::"),
            "{} lives in {}",
            method.name,
            method.module
        );
    }
}

#[test]
fn each_method_is_listed_once() {
    let mut names: Vec<&str> = METHODS.iter().map(|m| m.name).collect();
    names.sort_unstable();
    let count = names.len();
    names.dedup();
    assert_eq!(names.len(), count);
}
