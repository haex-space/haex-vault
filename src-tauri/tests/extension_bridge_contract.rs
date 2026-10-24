//! Spec 017, FR-009, T074: the allowlist of the bridge is the contract. Every method of
//! `bridge::dispatch::METHODS` is listed in `contracts/bridge.md` §Methoden, and no handler lives
//! under `chat`, `llm`, `adapters` or `providers`: an extension never reaches the model side of
//! holzi (ADR-0004).

use std::path::Path;

use holzi_lib::extensions::bridge::dispatch::METHODS;

fn methods_section() -> String {
    let contract = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../specs/017-extension-host/contracts/bridge.md");
    let text = std::fs::read_to_string(contract).expect("bridge contract");
    let start = text.find("## Methoden").expect("section Methoden");
    let rest = &text[start..];
    let end = rest[3..].find("\n## ").map_or(rest.len(), |i| i + 3);
    rest[..end].to_owned()
}

#[test]
fn every_method_of_the_allowlist_is_in_the_contract() {
    let section = methods_section();
    let missing: Vec<&str> = METHODS
        .iter()
        .map(|m| m.name)
        .filter(|name| !section.contains(&format!("`{name}`")))
        .collect();
    assert!(
        missing.is_empty(),
        "not in contracts/bridge.md: {missing:?}"
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
            method.module.starts_with("holzi_lib::extensions::"),
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

/// The methods of channels between extensions (spec 057): all in `extensions::peers::bridge`,
/// named by the 017 contract and by the contract of 057 it points to, with their events.
#[test]
fn the_methods_and_events_of_channels_are_in_both_contracts() {
    const EVENTS: [&str; 5] = [
        "peers:invite-status",
        "peers:channel-opened",
        "peers:joined",
        "peers:message",
        "peers:channel-closed",
    ];
    let peers: Vec<_> = METHODS
        .iter()
        .filter(|m| m.name.starts_with("extension_peers_"))
        .collect();
    assert_eq!(
        peers.len(),
        8,
        "{:?}",
        peers.iter().map(|m| m.name).collect::<Vec<_>>()
    );
    let own = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../specs/057-extension-peer-channels/contracts/bridge.md"),
    )
    .expect("the contract of 057");
    let host = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../specs/017-extension-host/contracts/bridge.md"),
    )
    .expect("the bridge contract");
    for method in peers {
        assert_eq!(
            method.module, "holzi_lib::extensions::peers::bridge",
            "{}",
            method.name
        );
        assert!(
            own.contains(&format!("`{}`", method.name)),
            "{}",
            method.name
        );
        assert!(
            host.contains(&format!("`{}`", method.name)),
            "017: {}",
            method.name
        );
    }
    for event in EVENTS {
        assert!(host.contains(&format!("`{event}`")), "017: {event}");
        assert!(own.contains(&format!("`{event}`")), "057: {event}");
    }
}
