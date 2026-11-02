//! Which model a session uses (spec 006, spec 002 FR-009 as amended 2026-11-02): a continued
//! session keeps the model it used while that model can load; a new session starts with the last
//! model picked in the model menu, then the defaults, then anything that loads.

use super::{choose_model, ResolveSource};

fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

fn some(id: &str) -> Option<String> {
    Some(id.to_owned())
}

#[test]
fn a_continued_session_keeps_its_model() {
    let chosen = choose_model(
        [some("claude:sonnet"), some("qwen3-4b"), None, None],
        ids(&["qwen3-4b"]),
        ids(&["claude:sonnet"]),
    );
    assert_eq!(chosen.model_id.as_deref(), Some("claude:sonnet"));
    assert_eq!(chosen.source, ResolveSource::Session);
}

#[test]
fn a_session_whose_model_is_gone_falls_back_to_the_last_pick() {
    let chosen = choose_model(
        [some("deleted-model"), some("qwen3-4b"), None, None],
        ids(&["qwen3-4b"]),
        Vec::new(),
    );
    assert_eq!(chosen.model_id.as_deref(), Some("qwen3-4b"));
    assert_eq!(chosen.source, ResolveSource::LastActive);
}

#[test]
fn a_new_session_starts_with_the_last_pick() {
    let chosen = choose_model(
        [None, some("codex:gpt"), some("qwen3-4b"), None],
        ids(&["qwen3-4b"]),
        ids(&["codex:gpt"]),
    );
    assert_eq!(chosen.model_id.as_deref(), Some("codex:gpt"));
    assert_eq!(chosen.source, ResolveSource::LastActive);
}

#[test]
fn defaults_follow_in_order() {
    let device = choose_model(
        [None, some("gone"), some("dev"), some("vault")],
        ids(&["dev", "vault"]),
        Vec::new(),
    );
    assert_eq!(device.source, ResolveSource::DefaultDevice);
    let vault = choose_model(
        [None, None, some("gone"), some("vault")],
        ids(&["vault"]),
        Vec::new(),
    );
    assert_eq!(vault.model_id.as_deref(), Some("vault"));
    assert_eq!(vault.source, ResolveSource::DefaultVault);
}

#[test]
fn without_preferences_a_local_model_comes_before_an_api_model() {
    let chosen = choose_model(
        [None, None, None, None],
        ids(&["local-a", "local-b"]),
        ids(&["claude:sonnet"]),
    );
    assert_eq!(chosen.model_id.as_deref(), Some("local-a"));
    assert_eq!(chosen.source, ResolveSource::FirstAvailable);
    let api_only = choose_model(
        [None, None, None, None],
        Vec::new(),
        ids(&["claude:sonnet"]),
    );
    assert_eq!(api_only.model_id.as_deref(), Some("claude:sonnet"));
    let nothing = choose_model([some("x"), None, None, None], Vec::new(), Vec::new());
    assert_eq!(nothing.model_id, None);
    assert_eq!(nothing.source, ResolveSource::None);
}
