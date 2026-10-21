//! Tests for approvals and blocks (spec 049, T017): the "never wider" check of an approval, the
//! decision for a handshake, the one grant of an approval and the stable row id. Reading and
//! writing the table runs in `tests/browser_bridge.rs` against a real vault.

use serde_json::json;
use uuid::Uuid;

use super::approvals::{
    decide, kept_name, row_id, validate, Approval, ClientIdentity, Decision, InvalidApproval,
    SessionApproval, SessionDecisions, Status, StoredClient,
};
use super::declaration::{BridgeAccess, Declared};
use crate::passwords::access::{GrantAction, Scope};

fn tags(list: &[&str]) -> Vec<String> {
    list.iter().map(|t| t.to_string()).collect()
}

fn everything() -> Declared {
    Declared::read(Some(&json!({
        "core": { "passwords": [{ "target": "*", "operation": "readWrite" }] }
    })))
    .expect("declared")
}

fn identity(key: &str) -> ClientIdentity {
    ClientIdentity {
        client_id: "fp".into(),
        public_key: key.into(),
        client_name: "haex-pass Browser Extension".into(),
        origin: "moz-extension://abc".into(),
        declaration: "{}".into(),
    }
}

fn read_web() -> Approval {
    Approval {
        access: BridgeAccess::Read,
        scope_tags: Some(tags(&["web"])),
        default_tag: None,
        write_denied: false,
    }
}

fn stored(status: Status) -> StoredClient {
    StoredClient {
        identity: identity("K"),
        status,
        decided_at: 1,
        last_seen_at: None,
    }
}

#[test]
fn an_approval_within_the_declaration_is_kept_with_clean_tags() {
    let approval = validate(
        &everything(),
        BridgeAccess::ReadWrite,
        Some(tags(&[" web ", "web", "shop", ""])),
        Some(" web ".into()),
    )
    .expect("valid");
    assert_eq!(approval.scope_tags, Some(tags(&["shop", "web"])));
    assert_eq!(approval.default_tag.as_deref(), Some("web"));
    assert!(!approval.write_denied);
}

#[test]
fn reading_and_writing_in_a_tag_scope_needs_a_default_tag_inside_it() {
    assert_eq!(
        validate(
            &everything(),
            BridgeAccess::ReadWrite,
            Some(tags(&["web"])),
            None
        ),
        Err(InvalidApproval::DefaultTag)
    );
    assert_eq!(
        validate(
            &everything(),
            BridgeAccess::ReadWrite,
            Some(tags(&["web"])),
            Some("shop".into())
        ),
        Err(InvalidApproval::DefaultTag)
    );
    // With every entry the default tag may stay empty (FR-010).
    assert!(validate(&everything(), BridgeAccess::ReadWrite, None, None).is_ok());
    // Reading needs none.
    assert!(validate(
        &everything(),
        BridgeAccess::Read,
        Some(tags(&["web"])),
        None
    )
    .is_ok());
}

#[test]
fn an_empty_tag_scope_or_a_wider_grant_is_refused() {
    assert_eq!(
        validate(&everything(), BridgeAccess::Read, Some(tags(&[" "])), None),
        Err(InvalidApproval::EmptyScope)
    );
    let read_only = Declared::read(Some(&json!({
        "core": { "passwords": [{ "target": "web", "operation": "read" }] }
    })))
    .expect("declared");
    assert_eq!(
        validate(
            &read_only,
            BridgeAccess::ReadWrite,
            Some(tags(&["web"])),
            Some("web".into())
        ),
        Err(InvalidApproval::WiderThanDeclared)
    );
    assert_eq!(
        validate(&read_only, BridgeAccess::Read, None, None),
        Err(InvalidApproval::WiderThanDeclared)
    );
}

#[test]
fn an_approval_is_one_grant() {
    let grants = read_web().grants();
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].action, GrantAction::Read);
    assert_eq!(grants[0].scope, Scope::tags(["web"]));
    let all = Approval {
        access: BridgeAccess::ReadWrite,
        scope_tags: None,
        default_tag: None,
        write_denied: false,
    };
    assert_eq!(all.grants()[0].action, GrantAction::ReadWrite);
    assert_eq!(all.grants()[0].scope, Scope::All);
}

#[test]
fn an_unknown_client_is_asked() {
    assert_eq!(
        decide(&identity("K"), None, &SessionDecisions::default()),
        Decision::Ask
    );
}

#[test]
fn a_remembered_approval_holds_only_for_the_same_identity() {
    let row = stored(Status::Allowed(read_web()));
    assert_eq!(
        decide(&identity("K"), Some(&row), &SessionDecisions::default()),
        Decision::Authorized {
            approval: read_web(),
            remembered: true
        }
    );
    // Another key, origin or declaration: asked again (FR-012, research R2).
    assert_eq!(
        decide(&identity("OTHER"), Some(&row), &SessionDecisions::default()),
        Decision::Ask
    );
    let mut moved = identity("K");
    moved.origin = "chrome-extension://xyz".into();
    assert_eq!(
        decide(&moved, Some(&row), &SessionDecisions::default()),
        Decision::Ask
    );
    let mut changed = identity("K");
    changed.declaration = r#"{"more":1}"#.into();
    assert_eq!(
        decide(&changed, Some(&row), &SessionDecisions::default()),
        Decision::Ask
    );
    // The claimed name is no part of the identity.
    let mut renamed = identity("K");
    renamed.client_name = "Other name".into();
    assert!(matches!(
        decide(&renamed, Some(&row), &SessionDecisions::default()),
        Decision::Authorized { .. }
    ));
}

#[test]
fn a_block_beats_every_approval() {
    let mut session = SessionDecisions::default();
    session.allow(SessionApproval {
        identity: identity("K"),
        approval: read_web(),
        decided_at: 1,
    });
    assert!(matches!(
        decide(&identity("K"), None, &session),
        Decision::Authorized {
            remembered: false,
            ..
        }
    ));
    assert_eq!(
        decide(&identity("K"), Some(&stored(Status::Blocked)), &session),
        Decision::Blocked
    );
    session.block(identity("K"), 2);
    assert_eq!(
        decide(
            &identity("K"),
            Some(&stored(Status::Allowed(read_web()))),
            &session
        ),
        Decision::Blocked
    );
    assert!(session.forget("fp"));
    assert!(!session.forget("fp"));
}

#[test]
fn the_row_id_is_stable_per_device_and_client() {
    let a = Uuid::from_u128(1);
    let b = Uuid::from_u128(2);
    assert_eq!(row_id(a, "fp"), row_id(a, "fp"));
    assert_ne!(row_id(a, "fp"), row_id(b, "fp"));
    assert_ne!(row_id(a, "fp"), row_id(a, "other"));
}

#[test]
fn the_claimed_name_is_trimmed_and_cut() {
    assert_eq!(kept_name(Some("  haex-pass  ")), "haex-pass");
    assert_eq!(kept_name(None), "");
    assert_eq!(kept_name(Some(&"x".repeat(300))).chars().count(), 100);
}
