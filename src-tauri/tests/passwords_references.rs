//! Integration coverage for references between entries against a real vault (spec 036, US7,
//! FR-044 to FR-049, `contracts/references.md`): reveal, copy and the single-entry read resolve; a
//! source outside a caller's scope or in the trash is absent for that caller with no hint; lists
//! for callers from outside never show a placeholder; the history keeps placeholders raw; a cycle
//! is refused on save; a missing source is allowed and marked; deleting a source for good can turn
//! the references into own values first.

// These tests read raw vault state that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;

use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};

use holzi_lib::identity::{
    haex_migration_source, installation_id_path, HaexBootstrap, HAEX_TRIGGER_VERSION,
};
use holzi_lib::passwords::access::{Caller, Grant, GrantAction, Scope};
use holzi_lib::passwords::model::{
    CopyField, ItemInput, ItemPatch, KeyValueInput, Patch, SecretField, Target, TargetKind,
    TextField,
};
use holzi_lib::passwords::model_references::{RefMarkKind, RefStatus};
use holzi_lib::passwords::service::{Headers, PasswordsService};
use holzi_lib::vault_gate::VaultGate;
use holzi_lib::HaexError;

const MARKER: &str = "SECRET-MARKER-REFERENCES";

struct Fixture {
    _dir: tempfile::TempDir,
    service: PasswordsService,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Database::open(DatabaseConfig {
        path: dir.path().join("vault.db"),
        key: SqlCipherKey::new("passwords-references"),
        create_if_missing: true,
        bootstrap: Arc::new(HaexBootstrap::new(installation_id_path(dir.path()))),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: haex_migration_source(),
        trigger_version: HAEX_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("open the vault");
    let vault_db = VaultGate::new()
        .vault_db(Arc::new(db))
        .expect("open the gate");
    Fixture {
        _dir: dir,
        service: PasswordsService::new(vault_db),
    }
}

fn source_input(tag: &str) -> ItemInput {
    ItemInput {
        title: Some("Konto".to_string()),
        username: Some("anna".to_string()),
        password: Some(format!("{MARKER}-password")),
        tags: vec![tag.to_string()],
        key_values: vec![KeyValueInput {
            key: "PIN".to_string(),
            value: Some(format!("{MARKER}-pin")),
        }],
        ..ItemInput::default()
    }
}

impl Fixture {
    async fn create(&self, input: ItemInput) -> String {
        self.service
            .create_item(&Caller::User, &[], input, None)
            .await
            .expect("create")
    }

    async fn token(&self, item_id: &str, kind: RefMarkKind, key: Option<&str>) -> String {
        self.service
            .reference_token(
                &Caller::User,
                item_id.to_string(),
                kind,
                key.map(str::to_string),
            )
            .await
            .expect("token")
    }

    async fn reveal_password(&self, item_id: &str) -> Result<String, HaexError> {
        self.service
            .reveal(&Caller::User, item_id.to_string(), SecretField::Password)
            .await
            .map(|secret| secret.value.to_string())
    }

    async fn updated_at(&self, item_id: &str) -> String {
        self.service
            .get_item(&Caller::User, item_id.to_string())
            .await
            .expect("get")
            .header
            .updated_at
            .expect("token")
    }

    async fn update(&self, item_id: &str, patch: ItemPatch) -> Result<String, HaexError> {
        let token = self.updated_at(item_id).await;
        self.service
            .update_item(&Caller::User, &[], item_id.to_string(), token, patch)
            .await
    }

    /// A source tagged `src` and a target tagged `dst` whose username and password point at it.
    async fn pair(&self) -> (String, String) {
        let source = self.create(source_input("src")).await;
        let username = self.token(&source, RefMarkKind::Username, None).await;
        let password = self.token(&source, RefMarkKind::Password, None).await;
        let target = self
            .create(ItemInput {
                title: Some("Zweit".to_string()),
                username: Some(username),
                password: Some(format!("x-{password}")),
                tags: vec!["dst".to_string()],
                ..ItemInput::default()
            })
            .await;
        (source, target)
    }
}

fn outside(tags: &[&str]) -> (Caller, Vec<Grant>) {
    (
        Caller::Extension {
            id: "ext".to_string(),
        },
        vec![Grant::new(
            GrantAction::Read,
            Scope::tags(tags.iter().copied()),
        )],
    )
}

#[tokio::test]
async fn a_text_copied_from_the_editor_or_a_history_state_resolves_its_placeholders() {
    let f = fixture();
    let source = f.create(source_input("src")).await;
    let password = f.token(&source, RefMarkKind::Password, None).await;
    let pin = f.token(&source, RefMarkKind::Extra, Some("PIN")).await;
    let resolve = |item_id: Option<String>, field: TextField, text: String| {
        f.service
            .resolve_text(&Caller::User, item_id, field, text.into())
    };
    // A new entry has no id yet; a placeholder to another entry still resolves.
    let copied = resolve(None, TextField::Password, format!("{password}-x"))
        .await
        .expect("resolve");
    assert_eq!(copied.as_str(), format!("{MARKER}-password-x"));
    let copied = resolve(
        Some(source.clone()),
        TextField::KeyValue {
            key: "Kopie".to_string(),
        },
        pin,
    )
    .await
    .expect("resolve");
    assert_eq!(copied.as_str(), format!("{MARKER}-pin"));
    // Text without a placeholder stays as it is.
    let copied = resolve(None, TextField::Note, "plain {$ text".to_string())
        .await
        .expect("plain");
    assert_eq!(copied.as_str(), "plain {$ text");
    // A placeholder that does not resolve is an error, never the text (FR-045).
    let missing = "00000000-0000-4000-8000-0000000000fe";
    assert!(resolve(
        None,
        TextField::Username,
        format!("{{${missing}:password}}")
    )
    .await
    .is_err());
    // Only the user copies.
    assert!(f
        .service
        .resolve_text(
            &Caller::Extension {
                id: "ext".to_string()
            },
            None,
            TextField::Note,
            "x".to_string().into(),
        )
        .await
        .is_err());
}

#[tokio::test]
async fn reveal_copy_and_the_single_entry_read_resolve_for_the_user() {
    let f = fixture();
    let (source, target) = f.pair().await;
    assert_eq!(
        f.reveal_password(&target).await.expect("reveal"),
        format!("x-{MARKER}-password")
    );
    let copied = f
        .service
        .copy_value(&Caller::User, target.clone(), CopyField::Username)
        .await
        .expect("copy");
    assert_eq!(copied.as_str(), "anna");

    // A change of the source shows in the target at once.
    f.update(
        &source,
        ItemPatch {
            password: Patch::Set("changed".to_string()),
            ..ItemPatch::default()
        },
    )
    .await
    .expect("update");
    assert_eq!(
        f.reveal_password(&target).await.expect("reveal"),
        "x-changed"
    );

    let (caller, grants) = outside(&["src", "dst"]);
    let item = f
        .service
        .read_secret_item(&caller, &grants, target.clone())
        .await
        .expect("read");
    assert_eq!(item.username.as_deref(), Some("anna"));
    assert_eq!(item.password.as_deref(), Some("x-changed"));
}

#[tokio::test]
async fn a_source_outside_the_scope_or_in_the_trash_is_absent_for_a_caller_from_outside() {
    let f = fixture();
    let (source, target) = f.pair().await;
    // The grant covers the target, not the source: the fields are absent, nothing hints why.
    let (caller, grants) = outside(&["dst"]);
    let item = f
        .service
        .read_secret_item(&caller, &grants, target.clone())
        .await
        .expect("read");
    assert_eq!(item.username, None);
    assert_eq!(item.password, None);
    assert!(!format!("{item:?}").contains(MARKER));

    // In the trash the source is still visible to the user, not to a caller from outside.
    f.service
        .trash_targets(
            &Caller::User,
            vec![Target {
                kind: TargetKind::Item,
                id: source.clone(),
            }],
        )
        .await
        .expect("trash");
    assert_eq!(
        f.reveal_password(&target).await.expect("reveal"),
        format!("x-{MARKER}-password")
    );
    let (caller, grants) = outside(&["src", "dst"]);
    let item = f
        .service
        .read_secret_item(&caller, &grants, target)
        .await
        .expect("read");
    assert_eq!(item.password, None);
}

#[tokio::test]
async fn lists_for_callers_from_outside_never_show_a_placeholder() {
    let f = fixture();
    let (_, target) = f.pair().await;
    let plain = f
        .create(ItemInput {
            title: Some("Plain".to_string()),
            username: Some("price {$ 5".to_string()),
            tags: vec!["dst".to_string()],
            ..ItemInput::default()
        })
        .await;
    let (caller, grants) = outside(&["dst"]);
    let Headers::Items(headers) = f
        .service
        .list_headers(&caller, &grants)
        .await
        .expect("list")
    else {
        panic!("items expected");
    };
    let header = headers.iter().find(|h| h.id == target).expect("target");
    assert_eq!(header.username, None, "the placeholder is blanked");
    assert!(header.has_password, "a placeholder counts as a password");
    let plain = headers.iter().find(|h| h.id == plain).expect("plain");
    assert_eq!(plain.username.as_deref(), Some("price {$ 5"));

    // The user's overview stays raw; the window shows marks.
    let overview = f
        .service
        .load_overview(&Caller::User)
        .await
        .expect("overview");
    let raw = overview
        .headers
        .iter()
        .find(|h| h.id == target)
        .expect("target");
    assert!(raw.username.as_deref().is_some_and(|u| u.starts_with("{$")));
}

#[tokio::test]
async fn the_history_keeps_the_placeholder_and_a_restore_brings_it_back() {
    let f = fixture();
    let (source, target) = f.pair().await;
    let placeholder = f.token(&source, RefMarkKind::Username, None).await;
    f.update(
        &target,
        ItemPatch {
            username: Patch::Set("someone".to_string()),
            ..ItemPatch::default()
        },
    )
    .await
    .expect("update");
    let states = f
        .service
        .history_list(&Caller::User, target.clone())
        .await
        .expect("history");
    let first = states.last().expect("the first state");
    let view = f
        .service
        .history_get(&Caller::User, first.id.clone())
        .await
        .expect("state");
    assert_eq!(view.username.as_deref(), Some(placeholder.as_str()));
    let token = f.updated_at(&target).await;
    f.service
        .history_restore(&Caller::User, target.clone(), first.id.clone(), token)
        .await
        .expect("restore");
    let detail = f
        .service
        .get_item(&Caller::User, target)
        .await
        .expect("get");
    assert_eq!(
        detail.header.username.as_deref(),
        Some(placeholder.as_str())
    );
    assert_eq!(detail.references.username[0].status, RefStatus::Ok);
}

#[tokio::test]
async fn a_cycle_is_refused_on_save_and_a_missing_source_is_allowed_and_marked() {
    let f = fixture();
    let (source, target) = f.pair().await;
    // Pointing the source's password back at the target's closes a cycle.
    let back = f.token(&target, RefMarkKind::Password, None).await;
    let refused = f
        .update(
            &source,
            ItemPatch {
                password: Patch::Set(back),
                ..ItemPatch::default()
            },
        )
        .await;
    assert!(
        matches!(&refused, Err(HaexError::PasswordsReferenceCycle { source_item_id }) if *source_item_id == target),
        "{refused:?}"
    );
    // A reference to the same entry's own custom field is fine.
    let own_pin = f.token(&source, RefMarkKind::Extra, Some("PIN")).await;
    f.update(
        &source,
        ItemPatch {
            password: Patch::Set(own_pin),
            ..ItemPatch::default()
        },
    )
    .await
    .expect("own field");
    assert_eq!(
        f.reveal_password(&target).await.expect("reveal"),
        format!("x-{MARKER}-pin")
    );

    let missing = "00000000-0000-4000-8000-0000000000ff";
    let entry = f
        .create(ItemInput {
            title: Some("Later".to_string()),
            password: Some(format!("{{${missing}:password}}")),
            ..ItemInput::default()
        })
        .await;
    let detail = f
        .service
        .get_item(&Caller::User, entry.clone())
        .await
        .expect("get");
    assert_eq!(detail.references.password[0].status, RefStatus::Missing);
    let error = f.reveal_password(&entry).await.expect_err("missing");
    assert!(matches!(&error, HaexError::PasswordsReference { reason } if reason == "missing"));
}

#[tokio::test]
async fn deleting_a_source_for_good_can_turn_references_into_own_values() {
    let f = fixture();
    let (source, target) = f.pair().await;
    let usage = f
        .service
        .reference_usage(&Caller::User, vec![source.clone()])
        .await
        .expect("usage");
    assert_eq!(usage[0].target_items, 1);
    let before = f
        .service
        .history_list(&Caller::User, target.clone())
        .await
        .expect("history")
        .len();
    let to_delete = vec![Target {
        kind: TargetKind::Item,
        id: source.clone(),
    }];
    f.service
        .trash_targets(&Caller::User, to_delete.clone())
        .await
        .expect("trash");
    f.service
        .delete_permanently(&Caller::User, to_delete, true)
        .await
        .expect("delete");
    let detail = f
        .service
        .get_item(&Caller::User, target.clone())
        .await
        .expect("get");
    assert_eq!(detail.header.username.as_deref(), Some("anna"));
    assert!(detail.references.password.is_empty());
    assert_eq!(
        f.reveal_password(&target).await.expect("reveal"),
        format!("x-{MARKER}-password")
    );
    let after = f
        .service
        .history_list(&Caller::User, target)
        .await
        .expect("history")
        .len();
    assert_eq!(
        after,
        before + 1,
        "the inlined values are a state of their own"
    );
}

#[tokio::test]
async fn without_inlining_a_deleted_source_is_an_error_never_the_placeholder() {
    let f = fixture();
    let (source, target) = f.pair().await;
    let to_delete = vec![Target {
        kind: TargetKind::Item,
        id: source,
    }];
    f.service
        .trash_targets(&Caller::User, to_delete.clone())
        .await
        .expect("trash");
    f.service
        .delete_permanently(&Caller::User, to_delete, false)
        .await
        .expect("delete");
    let error = f.reveal_password(&target).await.expect_err("missing");
    assert!(matches!(&error, HaexError::PasswordsReference { reason } if reason == "missing"));
    let copied = f
        .service
        .copy_value(&Caller::User, target, CopyField::Password)
        .await;
    assert!(copied.is_err(), "the placeholder is never copied");
    for text in [error.to_string(), format!("{error:?}")] {
        assert!(!text.contains(MARKER), "{text}");
        assert!(!text.contains("{$"), "{text}");
    }
}

#[tokio::test]
async fn a_restore_that_would_close_a_cycle_is_refused() {
    let f = fixture();
    let (source, target) = f.pair().await;
    f.update(
        &target,
        ItemPatch {
            password: Patch::Set("plain".to_string()),
            ..ItemPatch::default()
        },
    )
    .await
    .expect("plain");
    // Now the source may point at the target, which no longer points back.
    let back = f.token(&target, RefMarkKind::Password, None).await;
    f.update(
        &source,
        ItemPatch {
            password: Patch::Set(back),
            ..ItemPatch::default()
        },
    )
    .await
    .expect("back");
    let states = f
        .service
        .history_list(&Caller::User, target.clone())
        .await
        .expect("history");
    let first = states.last().expect("the first state");
    let token = f.updated_at(&target).await;
    let refused = f
        .service
        .history_restore(&Caller::User, target.clone(), first.id.clone(), token)
        .await;
    assert!(
        matches!(&refused, Err(HaexError::PasswordsReferenceCycle { .. })),
        "{refused:?}"
    );
    assert_eq!(f.reveal_password(&target).await.expect("reveal"), "plain");
}

#[tokio::test]
async fn a_caller_from_outside_cannot_point_at_an_entry_outside_its_scope() {
    let f = fixture();
    let (source, target) = f.pair().await;
    let password = f.token(&source, RefMarkKind::Password, None).await;
    let caller = Caller::Extension {
        id: "ext".to_string(),
    };
    let grants = vec![Grant::new(GrantAction::ReadWrite, Scope::tags(["dst"]))];
    let created = f
        .service
        .create_item(
            &caller,
            &grants,
            ItemInput {
                title: Some("Fremd".to_string()),
                password: Some(password.clone()),
                tags: vec!["dst".to_string()],
                ..ItemInput::default()
            },
            None,
        )
        .await;
    assert!(
        matches!(&created, Err(HaexError::PasswordsReference { reason }) if reason == "missing"),
        "{created:?}"
    );
    let token = f.updated_at(&target).await;
    let updated = f
        .service
        .update_item(
            &caller,
            &grants,
            target.clone(),
            token.clone(),
            ItemPatch {
                note: Patch::Set(password),
                ..ItemPatch::default()
            },
        )
        .await;
    assert!(
        matches!(&updated, Err(HaexError::PasswordsReference { reason }) if reason == "missing"),
        "{updated:?}"
    );
    // A change of another field is not blocked by the user's own references to the source.
    f.service
        .update_item(
            &caller,
            &grants,
            target,
            token,
            ItemPatch {
                note: Patch::Set("ok".to_string()),
                ..ItemPatch::default()
            },
        )
        .await
        .expect("other field");
}

#[tokio::test]
async fn a_user_name_with_a_reference_is_revealed_resolved() {
    let f = fixture();
    let (_source, target) = f.pair().await;
    let revealed = f
        .service
        .reveal(&Caller::User, target, SecretField::Username)
        .await
        .expect("reveal");
    assert_eq!(revealed.value.as_str(), "anna");
}
