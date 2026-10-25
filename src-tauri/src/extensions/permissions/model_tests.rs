//! Tests for the kinds `model` and `modelDestination` (spec 055 R3) and `peers` (spec 057): their
//! stored spelling, their actions and how a stored row reads.

use super::*;
use crate::model_jobs::effort::Level;

const PROVIDER: &str = "6f1c1d52-3a8b-4f0e-9c55-0a1b2c3d4e5f";

#[test]
fn the_two_kinds_have_a_stored_name_and_are_part_of_all() {
    assert_eq!(PermissionKind::ALL.len(), 14);
    assert_eq!(PermissionKind::Model.as_str(), "model");
    assert_eq!(
        PermissionKind::ModelDestination.as_str(),
        "modelDestination"
    );
    for kind in [PermissionKind::Model, PermissionKind::ModelDestination] {
        assert_eq!(PermissionKind::parse(kind.as_str()), Some(kind));
        assert!(PermissionKind::ALL.contains(&kind));
        // Vault-wide: a grant holds on every own device, nobody chooses.
        assert!(!kind.is_device_scoped());
    }
}

#[test]
fn a_model_action_is_an_effort_and_a_destination_action_is_send() {
    for (text, level) in [
        ("none", Level::None),
        ("low", Level::Low),
        ("medium", Level::Medium),
        ("high", Level::High),
        ("xhigh", Level::XHigh),
        ("max", Level::Max),
    ] {
        let action = Action::parse(PermissionKind::Model, text).expect("effort");
        assert_eq!(action, Action::Effort(level));
        assert_eq!(action.as_string(), text);
    }
    assert_eq!(
        Action::parse(PermissionKind::ModelDestination, "send"),
        Some(Action::Send)
    );
    // Nothing else is a model action, and an action of another kind is not one either.
    for text in ["", "minimal", "High", "read", "readWrite", "send", "*"] {
        assert_eq!(Action::parse(PermissionKind::Model, text), None, "{text}");
    }
    for text in ["read", "low", "*", "Send"] {
        assert_eq!(
            Action::parse(PermissionKind::ModelDestination, text),
            None,
            "{text}"
        );
    }
    assert_eq!(Action::parse(PermissionKind::Mail, "low"), None);
}

#[test]
fn a_higher_effort_covers_a_lower_one_and_never_the_other_way() {
    let order = [Level::None, Level::Low, Level::Medium, Level::High];
    for (i, granted) in order.into_iter().enumerate() {
        for (j, asked) in order.into_iter().enumerate() {
            assert_eq!(
                Action::Effort(granted).covers(&Action::Effort(asked)),
                i >= j,
                "{granted:?} covers {asked:?}"
            );
        }
    }
    // An effort covers no other kind of action, and `send` only itself.
    assert!(!Action::Effort(Level::High).covers(&Action::Read));
    assert!(Action::Send.covers(&Action::Send));
}

#[test]
fn a_stored_model_row_reads_back_and_a_malformed_one_is_absent() {
    let device = VAULT_WIDE;
    let row = Permission::from_row("model", "medium", "*", "granted", device).expect("row");
    assert_eq!(row.kind, PermissionKind::Model);
    assert_eq!(row.action, Action::Effort(Level::Medium));
    let destination =
        Permission::from_row("modelDestination", "send", PROVIDER, "granted", device).expect("row");
    assert_eq!(destination.kind, PermissionKind::ModelDestination);
    // A row holzi cannot read is absent, never another permission (FR-022).
    assert!(Permission::from_row("model", "minimal", "*", "granted", device).is_none());
    assert!(Permission::from_row("model", "low", "", "granted", device).is_none());
    assert!(Permission::from_row("modelDestination", "send", "*", "granted", device).is_none());
    assert!(
        Permission::from_row("modelDestination", "send", "not-a-uuid", "granted", device).is_none()
    );
}

#[test]
fn a_grant_for_a_model_covers_the_use_of_that_model_up_to_its_effort() {
    let grant = |action, target| {
        vec![Permission::from_row("model", action, target, "granted", VAULT_WIDE).expect("row")]
    };
    let ask = |action: Level, model: &str| PermissionRequest {
        kind: PermissionKind::Model,
        action: Action::Effort(action),
        target: RequestTarget::Model(model.to_owned()),
    };
    let device = VAULT_WIDE;

    let medium = grant("medium", "m1");
    assert_eq!(
        evaluate(&medium, &ask(Level::Low, "m1"), device),
        Decision::Allow
    );
    assert_eq!(
        evaluate(&medium, &ask(Level::High, "m1"), device),
        Decision::Prompt
    );
    assert_eq!(
        evaluate(&medium, &ask(Level::None, "m2"), device),
        Decision::Prompt
    );

    // `*` is every model.
    let any = grant("none", "*");
    assert_eq!(
        evaluate(&any, &ask(Level::None, "m2"), device),
        Decision::Allow
    );
    assert_eq!(
        evaluate(&any, &ask(Level::Low, "m2"), device),
        Decision::Prompt
    );

    // A denial of a model wins over a grant for every model.
    let mut both = grant("high", "*");
    both.push(Permission::from_row("model", "high", "m1", "denied", VAULT_WIDE).expect("row"));
    assert_eq!(
        evaluate(&both, &ask(Level::None, "m1"), device),
        Decision::Deny
    );
    assert_eq!(
        evaluate(&both, &ask(Level::None, "m2"), device),
        Decision::Allow
    );
}

#[test]
fn a_destination_grant_names_one_provider_and_nothing_else() {
    let granted =
        vec![
            Permission::from_row("modelDestination", "send", PROVIDER, "granted", VAULT_WIDE)
                .expect("row"),
        ];
    let ask = |provider: &str| PermissionRequest {
        kind: PermissionKind::ModelDestination,
        action: Action::Send,
        target: RequestTarget::Provider(provider.to_owned()),
    };
    assert_eq!(
        evaluate(&granted, &ask(PROVIDER), VAULT_WIDE),
        Decision::Allow
    );
    assert_eq!(
        evaluate(
            &granted,
            &ask("0a0a0a0a-0a0a-4a0a-8a0a-0a0a0a0a0a0a"),
            VAULT_WIDE
        ),
        Decision::Prompt
    );
}

#[test]
fn peers_is_a_vault_wide_kind_with_the_one_action_connect() {
    assert_eq!(PermissionKind::Peers.as_str(), "peers");
    assert_eq!(PermissionKind::parse("peers"), Some(PermissionKind::Peers));
    assert!(PermissionKind::ALL.contains(&PermissionKind::Peers));
    // FR-001: one grant holds on every own device.
    assert!(!PermissionKind::Peers.is_device_scoped());
    assert_eq!(
        PermissionKind::Peers.scope_on(uuid::Uuid::new_v4()),
        VAULT_WIDE
    );

    let connect = Action::parse(PermissionKind::Peers, "connect").expect("connect");
    assert_eq!(connect, Action::Connect);
    assert_eq!(connect.as_string(), "connect");
    // Any other action of `peers` is absent (spec 017 FR-022), and `connect` belongs to no other
    // kind.
    for text in ["", "*", "read", "readWrite", "send", "Connect", "connect "] {
        assert_eq!(Action::parse(PermissionKind::Peers, text), None, "{text}");
    }
    for kind in PermissionKind::ALL
        .into_iter()
        .filter(|kind| *kind != PermissionKind::Peers)
    {
        assert_ne!(
            Action::parse(kind, "connect"),
            Some(Action::Connect),
            "{kind:?}"
        );
    }
    // `connect` covers only itself.
    assert!(Action::Connect.covers(&Action::Connect));
    assert!(!Action::Connect.covers(&Action::Read));
    assert!(!Action::ReadWrite.covers(&Action::Connect));
}

#[test]
fn a_stored_peers_row_reads_back_only_with_connect_on_any() {
    let row = Permission::from_row("peers", "connect", "*", "granted", VAULT_WIDE).expect("row");
    assert_eq!(row.kind, PermissionKind::Peers);
    assert_eq!(row.action, Action::Connect);
    assert_eq!(row.scope, GrantScope::Vault);
    let request = PermissionRequest {
        kind: PermissionKind::Peers,
        action: Action::Connect,
        target: RequestTarget::Any,
    };
    assert!(row.applies_to(&request));
    for (action, target) in [("read", "*"), ("connect", "contacts"), ("connect", "")] {
        assert_eq!(
            Permission::from_row("peers", action, target, "granted", VAULT_WIDE),
            None,
            "{action} {target}"
        );
    }
}
