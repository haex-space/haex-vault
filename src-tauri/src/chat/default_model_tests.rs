//! Which model a session uses (spec 006, spec 002 FR-009 as amended 2026-11-02): a continued
//! session keeps the model it used while that model can load; a new session starts with the last
//! model picked in the model menu. There is no default and nothing is picked on the user's
//! behalf: without a loadable pick there is no model.

use super::{choose_model, local_pick_is_usable, ResolveSource};

fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

fn some(id: &str) -> Option<String> {
    Some(id.to_owned())
}

#[test]
fn a_continued_session_keeps_its_model() {
    let chosen = choose_model(
        [some("claude:sonnet"), some("qwen3-4b")],
        ids(&["qwen3-4b"]),
        ids(&["claude:sonnet"]),
    );
    assert_eq!(chosen.model_id.as_deref(), Some("claude:sonnet"));
    assert_eq!(chosen.source, ResolveSource::Session);
}

#[test]
fn a_session_whose_model_is_gone_falls_back_to_the_last_pick() {
    let chosen = choose_model(
        [some("deleted-model"), some("qwen3-4b")],
        ids(&["qwen3-4b"]),
        Vec::new(),
    );
    assert_eq!(chosen.model_id.as_deref(), Some("qwen3-4b"));
    assert_eq!(chosen.source, ResolveSource::LastActive);
}

#[test]
fn a_new_session_starts_with_the_last_pick() {
    let chosen = choose_model(
        [None, some("codex:gpt")],
        ids(&["qwen3-4b"]),
        ids(&["codex:gpt"]),
    );
    assert_eq!(chosen.model_id.as_deref(), Some("codex:gpt"));
    assert_eq!(chosen.source, ResolveSource::LastActive);
}

#[test]
fn without_a_loadable_pick_there_is_no_model() {
    // A loadable model that nobody picked stays unused.
    let chosen = choose_model([None, None], ids(&["qwen3-4b"]), ids(&["codex:gpt"]));
    assert_eq!(chosen.model_id, None);
    assert_eq!(chosen.source, ResolveSource::None);

    let gone = choose_model(
        [None, some("deleted-model")],
        ids(&["qwen3-4b"]),
        Vec::new(),
    );
    assert_eq!(gone.model_id, None);
    assert_eq!(gone.source, ResolveSource::None);
}

#[test]
fn a_local_pick_needs_no_loaded_model_only_the_active_choice_and_its_file() {
    // Nothing is in memory after a vault open; the pick still counts and loads on use.
    assert!(local_pick_is_usable(Some("qwen3-4b"), "qwen3-4b", true));
}

#[test]
fn a_local_pick_that_is_not_the_active_model_or_has_no_file_is_not_usable() {
    assert!(!local_pick_is_usable(Some("other"), "qwen3-4b", true));
    assert!(!local_pick_is_usable(None, "qwen3-4b", true));
    assert!(!local_pick_is_usable(Some("qwen3-4b"), "qwen3-4b", false));
}

#[test]
fn a_usable_local_pick_is_the_chats_model_after_a_vault_open() {
    let usable = local_pick_is_usable(Some("qwen3-4b"), "qwen3-4b", true);
    let loadable_local = if usable {
        ids(&["qwen3-4b"])
    } else {
        Vec::new()
    };
    let chosen = choose_model([None, some("qwen3-4b")], loadable_local, Vec::new());
    assert_eq!(chosen.model_id.as_deref(), Some("qwen3-4b"));
    assert_eq!(chosen.source, ResolveSource::LastActive);
}
