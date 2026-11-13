//! Integration coverage for the public, app-independent model-load lifecycle.
//!
//! The actual model resolver needs a live Tauri `AppHandle` and filesystem
//! model catalog, so the desktop scenarios remain in the quickstart. These
//! tests exercise the generation hand-off that the resolver relies on.

use holzi_lib::chat::session::{ChatState, ModelLoadStatus};

#[test]
fn a_vault_transition_resets_status_and_rejects_the_previous_load() {
    let chat = ChatState::new();
    let (old_generation, old_load) = chat.begin_model_load();
    assert!(chat.set_model_loading(
        old_generation,
        old_load,
        "old-model".into(),
        "Old model".into(),
        "loading".into(),
        None,
    ));

    let new_generation = chat.bump_vault_generation();

    assert!(matches!(
        chat.model_load_status(),
        ModelLoadStatus::Idle { vault_generation } if vault_generation == new_generation
    ));
    assert!(!chat.set_model_ready(
        old_generation,
        old_load,
        "old-model".into(),
        "Old model".into(),
    ));
}
