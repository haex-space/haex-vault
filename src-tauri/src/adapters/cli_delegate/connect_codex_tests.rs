//! Unit tests for `connect_codex.rs`'s pure device-auth prompt parser — the
//! text shape here is the real one captured live from
//! `codex login --device-auth` (research.md §5), not an invented fixture.

use super::connect_codex::{parse_device_auth_prompt, DeviceAuthPrompt};

const LIVE_OUTPUT: &str = "Welcome to Codex [v\u{1b}[90m0.147.0\u{1b}[0m]\r\n\u{1b}[90mOpenAI's command-line coding agent\u{1b}[0m\r\n\r\nFollow these steps to sign in with ChatGPT using device code authorization:\r\n\r\n1. Open this link in your browser and sign in to your account\r\n   \u{1b}[94mhttps://auth.openai.com/codex/device\u{1b}[0m\r\n\r\n2. Enter this one-time code \u{1b}[90m(expires in 15 minutes)\u{1b}[0m\r\n   \u{1b}[94mGL00-DVEC2\u{1b}[0m\r\n\r\n\u{1b}[90mContinue only if you started this login in Codex. If a website or another person gave you this code, cancel.\u{1b}[0m\r\n";

#[test]
fn parses_the_url_and_code_from_the_real_device_auth_screen() {
    let prompt = parse_device_auth_prompt(LIVE_OUTPUT).unwrap();
    assert_eq!(
        prompt,
        DeviceAuthPrompt {
            url: "https://auth.openai.com/codex/device".to_string(),
            code: "GL00-DVEC2".to_string(),
        }
    );
}

#[test]
fn no_url_yet_yields_none() {
    let partial = "Welcome to Codex [v0.147.0]\r\nOpenAI's command-line coding agent\r\n";
    assert_eq!(parse_device_auth_prompt(partial), None);
}

#[test]
fn url_without_a_code_line_yet_yields_none() {
    let partial = "1. Open this link in your browser and sign in to your account\r\n   https://auth.openai.com/codex/device\r\n";
    assert_eq!(parse_device_auth_prompt(partial), None);
}

#[test]
fn prose_lines_do_not_get_mistaken_for_the_device_code() {
    // "OpenAI's command-line coding agent" and similar prose must never
    // match `is_device_code` just because a naive check ignores whitespace.
    let text = "https://auth.openai.com/codex/device\nOpenAI's command-line coding agent\n";
    assert_eq!(parse_device_auth_prompt(text), None);
}
