//! Tests for the approval table of the browser tools (spec 065, T045, contracts/browser-tools.md
//! "Freigabe").

use super::prompts::{need, BrowserPromptKind, Effect, Need};
use crate::chat::tools::permission::PermissionMode;

#[test]
fn every_row_and_mode_of_the_approval_table() {
    use Effect::{Change, Read, Submit};
    use PermissionMode::{Auto, Manual, Plan};
    let table = [
        (Read, Manual, Need::Run),
        (Read, Auto, Need::Run),
        (Read, Plan, Need::Run),
        (Change, Manual, Need::Ask(BrowserPromptKind::Action)),
        (Change, Auto, Need::Run),
        (Change, Plan, Need::Refuse("plan_mode")),
        (Submit, Manual, Need::Ask(BrowserPromptKind::Submit)),
        (Submit, Auto, Need::Ask(BrowserPromptKind::Submit)),
        (Submit, Plan, Need::Refuse("plan_mode")),
    ];
    for (effect, mode, expected) in table {
        assert_eq!(need(mode, effect), expected, "{effect:?} in {mode:?}");
    }
}

#[test]
fn a_submit_asks_once_not_twice() {
    // In Manuell the submit question replaces the action question: the table gives one kind.
    assert_eq!(
        need(PermissionMode::Manual, Effect::Submit),
        Need::Ask(BrowserPromptKind::Submit)
    );
}
