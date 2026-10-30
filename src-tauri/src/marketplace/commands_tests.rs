//! The pure parts of the commands, and the guard of FR-030 (T024): nothing an agent or an extension
//! can call is named after the marketplace.

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

#[test]
fn a_failed_source_reads_as_the_error_kind_of_a_command() {
    let reason = |kind| match failure_error(&SourceFailure { kind, detail: None }) {
        HolziError::Marketplace { reason } => reason,
        other => panic!("{other:?}"),
    };
    assert_eq!(reason(SourceErrorKind::Timeout), kinds::SOURCE_TIMEOUT);
    assert_eq!(
        reason(SourceErrorKind::AuthRejected),
        kinds::SOURCE_AUTH_REJECTED
    );
    assert_eq!(
        reason(SourceErrorKind::CredentialsSyncing),
        kinds::CREDENTIALS_SYNCING
    );
}

#[test]
fn a_newer_catalog_call_aborts_the_one_that_is_running() {
    let state = MarketplaceState::default();
    let first = state.begin_catalog();
    let _second = state.begin_catalog();
    let aborted = futures::executor::block_on(Abortable::new(std::future::pending::<()>(), first));
    assert!(aborted.is_err(), "the first call was aborted by the second");
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Source files below `dir`, tests and test support left out.
fn sources_below(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources_below(&path, extension, out);
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let is_test =
            name.contains("_tests.") || name.contains("test_support") || name.contains(".test.");
        if name.ends_with(extension) && !is_test {
            out.push(path);
        }
    }
}

/// FR-030: no agent action, no MCP tool, no host method of an extension and no tool of an
/// extension's agent registration is named after the marketplace. If this fails, someone gave an
/// agent or an extension a way to query the catalog or to install: decide that on purpose, change
/// spec 069 FR-030 first, then this test.
#[test]
fn nothing_an_agent_or_an_extension_can_call_is_named_after_the_marketplace() {
    let root = manifest_dir();
    let mut files = Vec::new();
    for dir in [
        "src/mcp_server",
        "src/extensions/bridge",
        "src/extensions/agent_tools",
    ] {
        sources_below(&root.join(dir), ".rs", &mut files);
    }
    sources_below(&root.join("../src/lib/actions"), ".ts", &mut files);
    // The handlers that run the actions of the agents (`*ActionHandlers.ts`).
    if let Ok(entries) = fs::read_dir(root.join("../src/stores")) {
        files.extend(entries.flatten().map(|e| e.path()).filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with("ActionHandlers.ts"))
        }));
    }
    assert!(
        files.len() > 10,
        "the registries were found: {}",
        files.len()
    );
    let mut hits = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file).unwrap_or_default();
        for (number, line) in text.lines().enumerate() {
            if line.to_ascii_lowercase().contains("marketplace") {
                hits.push(format!("{}:{}", file.display(), number + 1));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "FR-030 of spec 069: agents and extensions must not reach the marketplace, found at {hits:?}"
    );
}
