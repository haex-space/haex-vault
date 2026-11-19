//! Disabling, keeping and deleting data across devices (spec 017, US7, T090, FR-007, FR-008,
//! FR-039).

// The tests read rows directly to observe each device.
#![allow(clippy::disallowed_methods)]

use super::test_support::*;
use super::*;
use crate::extensions::permissions::prompts::Question;
use crate::extensions::permissions::{PermissionKind, PermissionStatus};
use crate::extensions::registry::list::list;
use crate::extensions::registry::remove::{purge_kept_data, remove, set_enabled};

fn listed(
    node: &Node,
    extension: Uuid,
) -> Option<crate::extensions::registry::list::ExtensionSummary> {
    let me = node.me;
    node.vault
        .read_blocking(move |q| list(q, me).map_err(Into::into))
        .expect("list")
        .into_iter()
        .find(|e| e.id == extension.to_string())
}

#[test]
fn disabling_on_a_disables_on_b_keeps_the_data_and_enabling_starts_it_again() {
    let (a, b) = (Node::new(), Node::new());
    let ext = a.install(&bundle("1.0.0", &[INIT]));
    a.follow();
    a.write("INSERT INTO `{t}` (id, label) VALUES ('i1', 'first')");
    b.pull(&a);
    b.follow();
    assert!(ready(&b, ext));

    set_enabled(&a.vault, ext, false, now()).expect("disable");
    a.follow();
    b.pull(&a);
    let changed = b.follow();
    assert_eq!(b.status(ext).map(|(s, _)| s).as_deref(), Some("disabled"));
    assert!(changed.iter().any(|c| c.status == "disabled"));
    assert_eq!(
        b.count(&format!(
            "SELECT enabled FROM extensions WHERE id = '{ext}'"
        )),
        Some(0)
    );
    assert_eq!(
        b.count("SELECT COUNT(*) FROM `{t}`"),
        Some(1),
        "the data stays"
    );

    set_enabled(&a.vault, ext, true, now()).expect("enable");
    b.pull(&a);
    b.follow();
    assert!(ready(&b, ext));
}

#[test]
fn only_an_installed_extension_can_be_disabled_and_only_kept_data_purged() {
    let a = Node::new();
    let ext = a.install(&bundle("1.0.0", &[INIT]));
    assert!(matches!(
        purge_kept_data(&a.vault, ext, now()),
        Err(crate::error::HaexError::ExtensionNotFound)
    ));
    remove(&a.vault, ext, false, now()).expect("remove");
    assert!(matches!(
        set_enabled(&a.vault, ext, false, now()),
        Err(crate::error::HaexError::ExtensionNotFound)
    ));
    purge_kept_data(&a.vault, ext, now()).expect("purge");
    assert!(matches!(
        purge_kept_data(&a.vault, ext, now()),
        Err(crate::error::HaexError::ExtensionNotFound)
    ));
}

#[test]
fn kept_data_reaches_a_device_added_after_the_removal() {
    let (a, b) = (Node::new(), Node::new());
    let ext = a.install(&bundle("1.0.0", &[INIT]));
    a.follow();
    a.write("INSERT INTO `{t}` (id, label) VALUES ('i1', 'kept')");
    remove(&a.vault, ext, false, now()).expect("remove");
    a.follow();
    assert_eq!(
        a.count("SELECT COUNT(*) FROM extension_migrations"),
        Some(1),
        "the migrations stay vault data"
    );

    // B joins only now: it never ran the extension, yet takes over its rows.
    b.pull(&a);
    b.follow();
    b.pull(&a);
    assert!(b.has_items());
    assert_eq!(b.count("SELECT COUNT(*) FROM `{t}`"), Some(1));
    assert_eq!(b.parked(), 0);
    let entry = listed(&b, ext).expect("kept data is listed");
    assert_eq!(entry.state, "removed");
    assert!(entry.kept_data_bytes.is_some_and(|bytes| bytes > 0));
}

#[test]
fn deleting_kept_data_on_a_drops_the_tables_on_b_and_a_reinstall_starts_fresh() {
    let (a, b) = (Node::new(), Node::new());
    let v1 = bundle("1.0.0", &[INIT]);
    let ext = a.install(&v1);
    a.follow();
    a.write("INSERT INTO `{t}` (id, label) VALUES ('old', 'kept')");
    b.pull(&a);
    b.follow();
    remove(&a.vault, ext, false, now()).expect("remove");
    a.follow();
    b.pull(&a);
    b.follow();
    assert!(b.has_items(), "kept");

    purge_kept_data(&a.vault, ext, now()).expect("purge");
    a.follow();
    b.pull(&a);
    b.follow();
    assert!(!a.has_items() && !b.has_items());
    assert_eq!(b.journal(), 0);
    assert!(listed(&b, ext).is_none(), "nothing kept, nothing listed");

    a.install(&v1);
    a.follow();
    b.pull(&a);
    b.follow();
    assert!(ready(&b, ext));
    assert_eq!(b.count("SELECT COUNT(*) FROM `{t}`"), Some(0));
}

/// A decision held on `node` for `extension` for the rest of the process.
fn hold_decision(node: &Node, extension: Uuid) {
    let question = Question {
        extension_id: extension,
        kind: PermissionKind::Database,
        action: "read".into(),
        target: "*".into(),
    };
    node.host
        .permissions
        .hold(&question, PermissionStatus::Granted);
}

#[test]
fn disabling_or_removing_on_a_closes_the_notifications_and_mail_watches_on_b() {
    let (a, b) = (Node::new(), Node::new());
    let ext = a.install(&bundle("1.0.0", &[INIT]));
    a.follow();
    b.pull(&a);
    b.follow();
    assert!(ready(&b, ext));
    let open = |node: &Node| node.host.notifications.of_extension(ext).len();

    b.host.notifications.open_for_test(ext);
    b.host.mail_watches.insert_for_test(ext);
    hold_decision(&b, ext);
    set_enabled(&a.vault, ext, false, now()).expect("disable");
    b.pull(&a);
    b.follow();
    assert_eq!(open(&b), 0, "closed on b although it was disabled on a");
    assert_eq!(
        b.host.mail_watches.count(ext),
        0,
        "mail watches end on b, with the credentials they hold"
    );
    assert_eq!(b.host.permissions.held(ext).len(), 1, "kept while disabled");

    set_enabled(&a.vault, ext, true, now()).expect("enable");
    b.pull(&a);
    b.follow();
    b.host.notifications.open_for_test(ext);
    b.follow();
    assert_eq!(open(&b), 1, "a running extension keeps its notifications");

    remove(&a.vault, ext, false, now()).expect("remove");
    b.pull(&a);
    b.follow();
    assert_eq!(open(&b), 0, "closed on b although it was removed on a");
    assert!(
        b.host.permissions.held(ext).is_empty(),
        "forgotten once removed"
    );
}

#[test]
fn a_reconcile_that_read_the_registry_before_an_enable_keeps_what_the_extension_has() {
    let node = Node::new();
    let ext = node.install(&bundle("1.0.0", &[INIT]));
    node.follow();
    // The reconcile began while the extension was still disabled; it was enabled since.
    node.host.notifications.open_for_test(ext);
    hold_decision(&node, ext);
    stopped_here(&node.vault, &node.host, ext);
    assert_eq!(node.host.notifications.of_extension(ext).len(), 1);
    assert_eq!(node.host.permissions.held(ext).len(), 1);
}

#[test]
fn a_device_with_an_older_version_shows_as_behind_until_it_updated() {
    let (a, b) = (Node::new(), Node::new());
    let ext = a.install(&bundle("1.0.0", &[INIT]));
    a.follow();
    b.pull(&a);
    b.follow();
    b.install(&bundle("1.1.0", &[INIT, TAG]));
    b.follow();

    let devices = |node: &Node| {
        listed(node, ext)
            .expect("listed")
            .devices
            .into_iter()
            .map(|d| (d.this_device, d.version, d.behind))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        devices(&b),
        vec![
            (true, Some("1.1.0".to_owned()), false),
            (false, Some("1.0.0".to_owned()), true),
        ]
    );

    a.pull(&b);
    a.follow();
    b.pull(&a);
    assert_eq!(
        devices(&b),
        vec![
            (true, Some("1.1.0".to_owned()), false),
            (false, Some("1.1.0".to_owned()), false),
        ]
    );
}
