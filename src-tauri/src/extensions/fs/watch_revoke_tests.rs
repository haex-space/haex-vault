//! A changed permission holds at once for running watches too (spec 017, FR-020): a watch only a
//! permission allowed ends when no permission allows its folder any more, a watch a dialog choice
//! allowed ends with its frame instead (FR-048).

use super::test_support::setup;
use super::*;
use crate::extensions::bridge::dispatch::call;
use crate::extensions::commands::permissions::{remove, PermissionRemoveArgs};
use crate::extensions::permissions::prompts::Question;
use crate::extensions::permissions::PermissionStatus;

#[test]
fn a_denied_permission_ends_the_watch_it_allowed_and_leaves_a_chosen_folder_watched() {
    let s = setup();
    let ctx = s.frame("good-notes-like.xt");
    let extension_id = ctx.session.extension_id;
    s.grant(&ctx, "read", &s.root);
    let watched = s.root.join("watched");
    std::fs::create_dir(&watched).unwrap();
    let granted = json!({ "ruleId": "granted", "path": watched.to_string_lossy() });
    call(&ctx, "extension_filesystem_watch", &granted).unwrap();
    *lock(&s.dialogs.choice) = Some(s.outside.clone());
    call(&ctx, "extension_filesystem_select_folder", &json!({})).unwrap();
    let chosen = json!({ "ruleId": "chosen", "path": s.outside.to_string_lossy() });
    call(&ctx, "extension_filesystem_watch", &chosen).unwrap();

    end_revoked_watches(&s.vault, &s.host, s.device);
    assert!(
        s.host.fs.watches.running(extension_id, "granted"),
        "a permission that still holds keeps its watch"
    );

    s.remember(&ctx, "read", &s.root, "denied");
    end_revoked_watches(&s.vault, &s.host, s.device);
    assert!(!s.host.fs.watches.running(extension_id, "granted"));
    assert!(s.host.fs.watches.running(extension_id, "chosen"));
}

#[test]
fn removing_a_decision_held_in_memory_ends_the_watch_it_allowed() {
    let s = setup();
    let ctx = s.frame("good-notes-like.xt");
    let extension_id = ctx.session.extension_id;
    let target = s.outside.to_string_lossy().into_owned();
    let question = Question {
        extension_id,
        kind: PermissionKind::Filesystem,
        action: "read".to_owned(),
        target: target.clone(),
    };
    s.host
        .permissions
        .hold(&question, PermissionStatus::Granted);
    let watch = json!({ "ruleId": "held", "path": target });
    call(&ctx, "extension_filesystem_watch", &watch).unwrap();

    remove(
        &s.vault,
        &s.host,
        s.device,
        PermissionRemoveArgs {
            extension_id: extension_id.to_string(),
            permission_id: None,
            temporary_key: Some(format!("filesystem|read|{target}")),
        },
    )
    .unwrap();
    assert!(!s.host.fs.watches.running(extension_id, "held"));
}

#[test]
fn a_watch_call_checks_the_running_watches_again_once_its_own_runs() {
    // A revocation whose sweep ran while a watch was still starting missed that watch: the watch
    // call itself checks again, here seen on a watch the sweep never reached.
    let s = setup();
    let ctx = s.frame("good-notes-like.xt");
    let extension_id = ctx.session.extension_id;
    s.grant(&ctx, "read", &s.root);
    s.grant(&ctx, "read", &s.outside);
    let raced = s.root.join("raced");
    std::fs::create_dir(&raced).unwrap();
    let watch = |rule: &str, path: &std::path::Path| {
        call(
            &ctx,
            "extension_filesystem_watch",
            &json!({ "ruleId": rule, "path": path.to_string_lossy() }),
        )
    };
    watch("raced", &raced).unwrap();
    s.remember(&ctx, "read", &s.root, "denied");

    watch("next", &s.outside).unwrap();
    assert!(!s.host.fs.watches.running(extension_id, "raced"));
    assert!(s.host.fs.watches.running(extension_id, "next"));
}

#[test]
fn releasing_a_drive_ends_every_watch_below_it_also_a_chosen_one() {
    let s = setup();
    let ctx = s.frame("good-notes-like.xt");
    let extension_id = ctx.session.extension_id;
    s.grant(&ctx, "read", &s.root);
    let watched = s.root.join("watched");
    std::fs::create_dir(&watched).unwrap();
    let granted = json!({ "ruleId": "granted", "path": watched.to_string_lossy() });
    call(&ctx, "extension_filesystem_watch", &granted).unwrap();
    *lock(&s.dialogs.choice) = Some(s.outside.clone());
    call(&ctx, "extension_filesystem_select_folder", &json!({})).unwrap();
    let chosen = json!({ "ruleId": "chosen", "path": s.outside.to_string_lossy() });
    call(&ctx, "extension_filesystem_watch", &chosen).unwrap();

    let drive = std::fs::canonicalize(&s.outside).unwrap();
    assert_eq!(s.host.fs.watches.end_under(&drive), 1);
    assert!(!s.host.fs.watches.running(extension_id, "chosen"));
    assert!(s.host.fs.watches.running(extension_id, "granted"));
}
