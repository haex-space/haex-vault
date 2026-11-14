use serde_json::json;

use super::{MarkerTexts, Signal};

#[test]
fn marker_distinguishes_the_builtin_and_unnamed_external_agents() {
    let builtin = MarkerTexts::new("en", true, "");
    assert_eq!(builtin.text, "Built-in agent is steering this tab");

    let unnamed = MarkerTexts::new("en", false, "");
    assert_eq!(unnamed.text, "Agent is steering this tab");

    let named = MarkerTexts::new("en", false, "Extension X");
    assert_eq!(named.text, "Extension X is steering this tab");
    assert_eq!(named.paused_text, "You have this tab; Extension X waits");
}

#[test]
fn only_the_markers_binding_and_words_are_signals() {
    let call = |name: &str, payload: &str| json!({ "name": name, "payload": payload });
    assert_eq!(Signal::of(&call("__holzi", "stop")), Some(Signal::Stop));
    assert_eq!(
        Signal::of(&call("__holzi", "takeover")),
        Some(Signal::Takeover)
    );
    assert_eq!(Signal::of(&call("__holzi", "return")), Some(Signal::Return));
    assert_eq!(Signal::of(&call("__holzi", "input")), Some(Signal::Input));
    assert_eq!(Signal::of(&call("__holzi", "hello")), Some(Signal::Hello));
    assert_eq!(
        Signal::of(&call("__holzi", "fullscreen")),
        Some(Signal::Fullscreen)
    );
    assert_eq!(
        Signal::of(&call("__holzi", "consent:declined")),
        Some(Signal::ConsentDeclined)
    );
    assert_eq!(
        Signal::of(&call("__holzi", "consent:open")),
        Some(Signal::ConsentOpen)
    );
    assert_eq!(
        Signal::of(&call("__holzi", "consent:wall")),
        Some(Signal::ConsentWall)
    );
    assert_eq!(Signal::of(&call("__holzi", "steal")), None);
    assert_eq!(Signal::of(&call("other", "stop")), None);
}

#[test]
fn the_script_carries_the_state() {
    let texts = MarkerTexts::new("de", false, "X");
    assert!(texts.script(true).contains(r#""paused":true"#));
    assert!(texts.script(false).contains(r#""paused":false"#));
}

#[test]
fn a_modifier_key_alone_is_not_the_user_working_in_the_page() {
    // Alt+Tab or the Super key to leave the browser reaches the focused page as a trusted key press;
    // both listeners (the main frame's and an embedded frame's) ask `byUser` first.
    let script = MarkerTexts::new("de", false, "X").script(false);
    for key in ["Alt", "Control", "Meta", "Shift", "Super"] {
        assert!(
            script.contains(&format!("'{key}'")),
            "{key} is not a modifier"
        );
    }
    assert_eq!(script.matches("byUser(event)").count(), 2);
    assert!(!script.contains("if (event.isTrusted) globalThis.__holzi('input')"));
}

#[test]
fn the_click_that_brings_the_browser_to_the_front_is_not_the_user_working_in_the_page() {
    let script = MarkerTexts::new("de", false, "X").script(false);
    assert!(script.contains("FOCUS_CLICK_MS"));
    assert!(script.contains("'focus'"));
    // Set at the start and spent by the click it covers; the behaviour itself runs in
    // `scripts/check-agent-browser-marker.ts`.
    assert_eq!(script.matches("__holziFocusedAt = -Infinity").count(), 2);
    // Once per document, and only the window's own focus: a field that gets the focus is no focus
    // click.
    assert!(script.contains("globalThis.__holziFocusedAt === undefined"));
    assert!(script.contains("event.target === window"));
}
