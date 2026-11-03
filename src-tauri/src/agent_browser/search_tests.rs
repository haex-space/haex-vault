//! `browser.search` becomes `browser.open` of the engine's result page (spec 065 FR-039).

use super::*;

#[test]
fn a_query_becomes_the_encoded_result_page_of_the_engine() {
    let open = as_open(
        DEFAULT_ENGINE,
        json!({ "query": "  Wetter Berlin & Umland  " }),
    )
    .expect("a search");
    assert_eq!(
        open,
        json!({ "url": "https://html.duckduckgo.com/html/?q=Wetter+Berlin+%26+Umland" })
    );
}

#[test]
fn an_empty_long_or_odd_query_is_refused() {
    for bad in [
        json!({ "query": "   " }),
        json!({ "query": "x".repeat(501) }),
        json!({ "q": "x" }),
        json!({ "query": "x", "url": "https://evil.example" }),
    ] {
        let refused = as_open(DEFAULT_ENGINE, bad.clone()).expect_err("refused");
        assert_eq!(refused.code, "invalid_input", "{bad}");
    }
}
