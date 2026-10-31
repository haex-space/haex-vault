//! Integration coverage for the access rules of the password manager against a real vault (spec
//! 034, US6, `contracts/access.md`): lists carry no secret for anyone, a read grant for a tag
//! reaches only the entries with that tag, a write grant cannot write an entry out of its scope or
//! into another's, tags outside the scope survive an update, the built-in agent sees titles only,
//! and an entry in the trash does not exist for callers from outside.

// These tests read raw vault state that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;

use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};

use holzi_lib::identity::{
    holzi_migration_source, installation_id_path, HolziBootstrap, HOLZI_TRIGGER_VERSION,
};
use holzi_lib::passwords::access::{Caller, Grant, GrantAction, Scope};
use holzi_lib::passwords::model::{ItemInput, ItemPatch, KeyValueInput, Patch};
use holzi_lib::passwords::service::{Headers, PasswordsService};
use holzi_lib::vault_gate::VaultGate;
use holzi_lib::HolziError;

const MARKER: &str = "SECRET-MARKER-ACCESS";

struct Fixture {
    _dir: tempfile::TempDir,
    db: Database,
    service: PasswordsService,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Database::open(DatabaseConfig {
        path: dir.path().join("vault.db"),
        key: SqlCipherKey::new("passwords-access"),
        create_if_missing: true,
        bootstrap: Arc::new(HolziBootstrap::new(installation_id_path(dir.path()))),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: holzi_migration_source(),
        trigger_version: HOLZI_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("open the vault");
    let vault_db = VaultGate::new()
        .vault_db(Arc::new(db.clone()))
        .expect("open the gate");
    Fixture {
        _dir: dir,
        db,
        service: PasswordsService::new(vault_db),
    }
}

fn tags(list: &[&str]) -> Vec<String> {
    list.iter().map(|t| t.to_string()).collect()
}

fn entry(title: &str, tag_names: &[&str]) -> ItemInput {
    ItemInput {
        title: Some(title.to_string()),
        username: Some(format!("user-of-{title}")),
        url: Some(format!("https://{title}.example.invalid")),
        password: Some(format!("{MARKER}-password-{title}")),
        note: Some(format!("{MARKER}-note-{title}")),
        otp_secret: Some("JBSWY3DPEHPK3PXP".to_string()),
        tags: tags(tag_names),
        key_values: vec![KeyValueInput {
            key: "PIN".to_string(),
            value: Some(format!("{MARKER}-pin-{title}")),
        }],
        ..ItemInput::default()
    }
}

struct Seeded {
    s3: String,
    bank: String,
    both: String,
    untagged: String,
}

async fn seed(f: &Fixture) -> Seeded {
    let create = |input: ItemInput| async {
        f.service
            .create_item(&Caller::User, &[], input, None)
            .await
            .expect("seed")
    };
    Seeded {
        s3: create(entry("s3", &["s3"])).await,
        bank: create(entry("bank", &["bank"])).await,
        both: create(entry("both", &["s3", "bank"])).await,
        untagged: create(entry("plain", &[])).await,
    }
}

fn read_grant(tag: &str) -> Vec<Grant> {
    vec![Grant::new(GrantAction::Read, Scope::tags([tag]))]
}

fn write_grant(tag: &str) -> Vec<Grant> {
    vec![Grant::new(GrantAction::ReadWrite, Scope::tags([tag]))]
}

fn outside_callers() -> Vec<Caller> {
    vec![
        Caller::Extension {
            id: "ext".to_string(),
        },
        Caller::ExternalAgent {
            id: "agent".to_string(),
        },
        Caller::Internal { feature: "test" },
        // Spec 049: a browser extension through the External Bridge.
        Caller::BrowserClient {
            id: "3f1c0000000000000000000000000000".to_string(),
        },
    ]
}

fn items_of(headers: Headers) -> Vec<holzi_lib::passwords::model::ItemHeader> {
    match headers {
        Headers::Items(items) => items,
        Headers::Agent(_) => panic!("expected item headers"),
    }
}

#[tokio::test]
async fn no_list_carries_a_secret_for_any_caller_not_even_after_a_thousand_calls() {
    let f = fixture();
    seed(&f).await;
    let everyone = {
        let mut all = outside_callers();
        all.push(Caller::BuiltinAgent);
        all.push(Caller::User);
        all
    };
    let grants = [Grant::new(GrantAction::ReadWrite, Scope::All)];
    for round in 0..1000 {
        for caller in &everyone {
            let headers = f.service.list_headers(caller, &grants).await.expect("list");
            let text = match &headers {
                Headers::Items(items) => serde_json::to_string(items).expect("json"),
                Headers::Agent(items) => serde_json::to_string(items).expect("json"),
            };
            assert!(!text.contains(MARKER), "round {round}: {text}");
            assert!(!text.contains("JBSWY3DPEHPK3PXP"), "{text}");
        }
        if round == 0 {
            // The first round also checks the debug print.
            let headers = f
                .service
                .list_headers(&Caller::User, &grants)
                .await
                .expect("list");
            assert!(!format!("{headers:?}").contains(MARKER));
        }
    }
}

#[tokio::test]
async fn a_read_grant_for_a_tag_reaches_only_the_entries_with_that_tag() {
    let f = fixture();
    let seeded = seed(&f).await;
    for caller in outside_callers() {
        let grants = read_grant("s3");
        let listed = items_of(
            f.service
                .list_headers(&caller, &grants)
                .await
                .expect("list"),
        );
        let mut titles: Vec<_> = listed.iter().filter_map(|h| h.title.clone()).collect();
        titles.sort();
        assert_eq!(titles, ["both", "s3"], "{caller:?}");

        let secret = f
            .service
            .read_secret_item(&caller, &grants, seeded.s3.clone())
            .await
            .expect("covered");
        assert_eq!(
            secret.password.as_deref(),
            Some(&*format!("{MARKER}-password-s3"))
        );
        assert_eq!(
            secret.key_values[0].value.as_deref(),
            Some(&*format!("{MARKER}-pin-s3"))
        );

        // Outside the scope, untagged and missing all give the same answer.
        let outside = [
            seeded.bank.clone(),
            seeded.untagged.clone(),
            "missing".to_string(),
        ];
        for id in outside {
            let result = f.service.read_secret_item(&caller, &grants, id).await;
            assert!(
                matches!(result, Err(HolziError::PasswordsNotFound)),
                "{result:?}"
            );
        }

        // A read grant never writes.
        let create = f
            .service
            .create_item(&caller, &grants, entry("new", &["s3"]), None)
            .await;
        assert!(matches!(create, Err(HolziError::PasswordsForbidden)));
        let update = f
            .service
            .update_item(
                &caller,
                &grants,
                seeded.s3.clone(),
                "x".to_string(),
                ItemPatch::default(),
            )
            .await;
        assert!(matches!(update, Err(HolziError::PasswordsForbidden)));
    }
}

#[tokio::test]
async fn without_any_grant_everything_is_forbidden_and_nothing_is_revealed() {
    let f = fixture();
    let seeded = seed(&f).await;
    for caller in outside_callers() {
        assert!(matches!(
            f.service.list_headers(&caller, &[]).await,
            Err(HolziError::PasswordsForbidden)
        ));
        // Not even whether an entry exists.
        for id in [seeded.s3.clone(), "missing".to_string()] {
            let result = f.service.read_secret_item(&caller, &[], id).await;
            assert!(
                matches!(result, Err(HolziError::PasswordsForbidden)),
                "{result:?}"
            );
        }
    }
}

#[tokio::test]
async fn a_write_grant_creates_only_entries_with_tags_of_its_scope() {
    let f = fixture();
    seed(&f).await;
    for caller in outside_callers() {
        let grants = write_grant("s3");
        let before = f
            .service
            .load_overview(&Caller::User)
            .await
            .expect("overview")
            .headers
            .len();
        let id = f
            .service
            .create_item(&caller, &grants, entry("fresh", &["s3"]), None)
            .await
            .expect("in scope");
        for bad in [&["s3", "other"][..], &["other"][..], &[][..]] {
            let result = f
                .service
                .create_item(&caller, &grants, entry("bad", bad), None)
                .await;
            assert!(
                matches!(result, Err(HolziError::PasswordsForbidden)),
                "{bad:?}: {result:?}"
            );
        }
        let after = f
            .service
            .load_overview(&Caller::User)
            .await
            .expect("overview")
            .headers
            .len();
        assert_eq!(after, before + 1, "only the in-scope create wrote");
        // The user sees the entry and can edit it (US6 scenario 8).
        let detail = f
            .service
            .get_item(&Caller::User, id.clone())
            .await
            .expect("user sees it");
        let token = detail.header.updated_at.expect("token");
        f.service
            .update_item(
                &Caller::User,
                &[],
                id,
                token,
                ItemPatch {
                    title: Patch::Set("edited by the user".to_string()),
                    ..ItemPatch::default()
                },
            )
            .await
            .expect("user edits");
    }
}

#[tokio::test]
async fn an_update_keeps_the_tags_outside_the_scope_and_cannot_leave_or_lift_the_entry() {
    let f = fixture();
    let seeded = seed(&f).await;
    let caller = Caller::Extension {
        id: "ext".to_string(),
    };
    let grants = write_grant("s3");
    let token = || async {
        f.service
            .get_item(&Caller::User, seeded.both.clone())
            .await
            .expect("detail")
            .header
            .updated_at
            .expect("token")
    };
    let tag_names = || async {
        let mut names: Vec<_> = f
            .service
            .get_item(&Caller::User, seeded.both.clone())
            .await
            .expect("detail")
            .header
            .tags
            .into_iter()
            .map(|t| t.name)
            .collect();
        names.sort();
        names
    };
    // The caller sees both tags.
    let listed = items_of(
        f.service
            .list_headers(&caller, &grants)
            .await
            .expect("list"),
    );
    let both = listed.iter().find(|h| h.id == seeded.both).expect("listed");
    assert_eq!(both.tags.len(), 2);

    // Sending only the in-scope tag keeps `bank`.
    f.service
        .update_item(
            &caller,
            &grants,
            seeded.both.clone(),
            token().await,
            ItemPatch {
                tags: Some(tags(&["s3"])),
                title: Patch::Set("renamed".to_string()),
                ..ItemPatch::default()
            },
        )
        .await
        .expect("update");
    assert_eq!(tag_names().await, ["bank", "s3"]);

    // Adding a tag outside the scope is refused and changes nothing.
    let lift = f
        .service
        .update_item(
            &caller,
            &grants,
            seeded.both.clone(),
            token().await,
            ItemPatch {
                tags: Some(tags(&["s3", "other"])),
                title: Patch::Set("must not be written".to_string()),
                ..ItemPatch::default()
            },
        )
        .await;
    assert!(matches!(lift, Err(HolziError::PasswordsForbidden)));

    // Removing the last in-scope tag is refused and leaves the entry unchanged.
    let leave = f
        .service
        .update_item(
            &caller,
            &grants,
            seeded.both.clone(),
            token().await,
            ItemPatch {
                tags: Some(tags(&["bank"])),
                title: Patch::Set("must not be written".to_string()),
                ..ItemPatch::default()
            },
        )
        .await;
    assert!(matches!(leave, Err(HolziError::PasswordsForbidden)));
    assert_eq!(tag_names().await, ["bank", "s3"]);
    let detail = f
        .service
        .get_item(&Caller::User, seeded.both.clone())
        .await
        .expect("detail");
    assert_eq!(detail.header.title.as_deref(), Some("renamed"));

    // An entry outside the scope is not there.
    let outside = f
        .service
        .update_item(
            &caller,
            &grants,
            seeded.bank.clone(),
            "x".to_string(),
            ItemPatch::default(),
        )
        .await;
    assert!(matches!(outside, Err(HolziError::PasswordsNotFound)));
}

#[tokio::test]
async fn the_built_in_agent_sees_titles_tags_and_folders_only_and_nothing_else() {
    let f = fixture();
    let seeded = seed(&f).await;
    let grants = [Grant::new(GrantAction::ReadWrite, Scope::All)];
    let Headers::Agent(list) = f
        .service
        .list_headers(&Caller::BuiltinAgent, &grants)
        .await
        .expect("agent list")
    else {
        panic!("expected agent headers");
    };
    assert_eq!(list.len(), 4);
    let json = serde_json::to_string(&list).expect("json");
    for hidden in ["user-of-", "example.invalid", MARKER, "JBSWY"] {
        assert!(!json.contains(hidden), "{hidden} in {json}");
    }
    let both = list.iter().find(|h| h.id == seeded.both).expect("listed");
    assert_eq!(both.title.as_deref(), Some("both"));
    assert!(both.has_totp);
    // Every other method is closed to it, whatever the grants say.
    assert!(matches!(
        f.service
            .read_secret_item(&Caller::BuiltinAgent, &grants, seeded.s3.clone())
            .await,
        Err(HolziError::PasswordsForbidden)
    ));
    assert!(matches!(
        f.service
            .create_item(&Caller::BuiltinAgent, &grants, entry("x", &["s3"]), None)
            .await,
        Err(HolziError::PasswordsForbidden)
    ));
    assert!(matches!(
        f.service.load_overview(&Caller::BuiltinAgent).await,
        Err(HolziError::PasswordsForbidden)
    ));
    assert!(matches!(
        f.service
            .reveal(
                &Caller::BuiltinAgent,
                seeded.s3.clone(),
                holzi_lib::passwords::model::SecretField::Password
            )
            .await,
        Err(HolziError::PasswordsForbidden)
    ));
}

#[tokio::test]
async fn an_entry_in_the_trash_does_not_exist_for_callers_from_outside() {
    let f = fixture();
    let seeded = seed(&f).await;
    // Put the s3 entry into the trash the way the trash feature will: a folder row with the fixed
    // id and the entry's folder link.
    f.db.write(|tx| {
        tx.execute(
            "INSERT INTO haex_passwords_groups (id) VALUES ('trash')",
            &[],
        )?;
        tx.execute(
            "INSERT INTO haex_passwords_group_items (item_id, group_id) VALUES (?1, 'trash')",
            &[&seeded.s3 as &dyn haex_crdt::rusqlite::ToSql],
        )?;
        Ok(())
    })
    .expect("trash it");
    for caller in outside_callers() {
        let grants = write_grant("s3");
        let listed = items_of(
            f.service
                .list_headers(&caller, &grants)
                .await
                .expect("list"),
        );
        assert!(
            listed.iter().all(|h| h.id != seeded.s3),
            "the trash is not listed"
        );
        assert!(matches!(
            f.service
                .read_secret_item(&caller, &grants, seeded.s3.clone())
                .await,
            Err(HolziError::PasswordsNotFound)
        ));
        assert!(matches!(
            f.service
                .update_item(
                    &caller,
                    &grants,
                    seeded.s3.clone(),
                    "x".to_string(),
                    ItemPatch::default()
                )
                .await,
            Err(HolziError::PasswordsNotFound)
        ));
    }
    // The user still reaches it, and it is unchanged.
    let detail = f
        .service
        .get_item(&Caller::User, seeded.s3)
        .await
        .expect("user");
    assert_eq!(detail.header.title.as_deref(), Some("s3"));
}

#[tokio::test]
async fn everything_beyond_the_item_methods_is_closed_to_callers_from_outside() {
    let f = fixture();
    let seeded = seed(&f).await;
    let all = [Grant::new(GrantAction::ReadWrite, Scope::All)];
    for caller in outside_callers() {
        assert!(matches!(
            f.service.load_overview(&caller).await,
            Err(HolziError::PasswordsForbidden)
        ));
        assert!(matches!(
            f.service.get_item(&caller, seeded.s3.clone()).await,
            Err(HolziError::PasswordsForbidden)
        ));
        assert!(matches!(
            f.service
                .set_tags(&caller, vec![seeded.s3.clone()], tags(&["x"]), vec![])
                .await,
            Err(HolziError::PasswordsForbidden)
        ));
        // Without a grant also the TOTP code and the presets stay closed (spec 049 FR-024,
        // FR-027 open them for a grant).
        assert!(matches!(
            f.service.totp_code(&caller, &[], seeded.s3.clone()).await,
            Err(HolziError::PasswordsForbidden)
        ));
        assert!(matches!(
            f.service.preset_list(&caller, &[]).await,
            Err(HolziError::PasswordsForbidden)
        ));
        // A grant for everything does not change that.
        let _ = &all;
    }
}

#[tokio::test]
async fn the_agent_search_filters_by_words_tag_and_limit_and_shows_no_username() {
    let f = fixture();
    seed(&f).await;
    let agent = Caller::BuiltinAgent;
    let all = f
        .service
        .agent_search(&agent, &[], None, None, None)
        .await
        .expect("all");
    assert_eq!(all.len(), 4);
    let by_word = f
        .service
        .agent_search(&agent, &[], Some("BANK".to_string()), None, None)
        .await
        .expect("by word");
    let mut titles: Vec<_> = by_word.iter().filter_map(|h| h.title.clone()).collect();
    titles.sort();
    assert_eq!(
        titles,
        ["bank", "both"],
        "the word is found in a title and in a tag name"
    );
    let by_tag = f
        .service
        .agent_search(&agent, &[], None, Some("S3".to_string()), None)
        .await
        .expect("by tag");
    assert_eq!(by_tag.len(), 2);
    let limited = f
        .service
        .agent_search(&agent, &[], None, None, Some(1))
        .await
        .expect("limited");
    assert_eq!(limited.len(), 1);
    let json = serde_json::to_string(&all).expect("json");
    for hidden in ["user-of-", "example.invalid", MARKER] {
        assert!(!json.contains(hidden), "{hidden} in {json}");
    }
    // Only the built-in agent searches this way.
    for caller in outside_callers() {
        let result = f.service.agent_search(&caller, &[], None, None, None).await;
        assert!(
            matches!(result, Err(HolziError::PasswordsForbidden)),
            "{caller:?}"
        );
    }
    let user = f
        .service
        .agent_search(&Caller::User, &[], None, None, None)
        .await;
    assert!(matches!(user, Err(HolziError::PasswordsForbidden)));
}

/// Spec 021 US5: an external agent searches only the entries its read grant covers, with the same
/// narrow headers; without a grant it gets nothing.
#[tokio::test]
async fn an_external_agent_searches_only_the_entries_of_its_grant() {
    let f = fixture();
    seed(&f).await;
    let agent = Caller::ExternalAgent {
        id: "claude".to_string(),
    };
    let grant = [Grant::new(GrantAction::Read, Scope::tags(["S3"]))];
    let found = f
        .service
        .agent_search(&agent, &grant, None, None, None)
        .await
        .expect("in scope");
    assert_eq!(found.len(), 2);
    assert!(found
        .iter()
        .all(|header| header.tags.iter().any(|tag| tag.eq_ignore_ascii_case("s3"))));
    let json = serde_json::to_string(&found).expect("json");
    for hidden in ["user-of-", "example.invalid", MARKER] {
        assert!(!json.contains(hidden), "{hidden} in {json}");
    }
    let refused = f.service.agent_search(&agent, &[], None, None, None).await;
    assert!(matches!(refused, Err(HolziError::PasswordsForbidden)));
}

fn id_target(id: &str) -> holzi_lib::passwords::model::Target {
    holzi_lib::passwords::model::Target {
        kind: holzi_lib::passwords::model::TargetKind::Item,
        id: id.to_string(),
    }
}

#[tokio::test]
async fn delete_needs_a_write_grant_and_an_entry_in_scope_and_only_moves_to_the_trash() {
    let f = fixture();
    let seeded = seed(&f).await;
    for caller in outside_callers() {
        let id = f
            .service
            .create_item(&caller, &write_grant("s3"), entry("victim", &["s3"]), None)
            .await
            .expect("create");
        // A read grant does not delete; outside the scope it is not there.
        assert!(matches!(
            f.service
                .delete_item(&caller, &read_grant("s3"), id.clone())
                .await,
            Err(HolziError::PasswordsForbidden)
        ));
        assert!(matches!(
            f.service
                .delete_item(&caller, &write_grant("s3"), seeded.bank.clone())
                .await,
            Err(HolziError::PasswordsNotFound)
        ));
        assert!(matches!(
            f.service
                .delete_item(&caller, &write_grant("s3"), "missing".to_string())
                .await,
            Err(HolziError::PasswordsNotFound)
        ));
        // In scope: the entry ends up in the trash with its data intact.
        f.service
            .delete_item(&caller, &write_grant("s3"), id.clone())
            .await
            .expect("delete");
        let overview = f
            .service
            .load_overview(&Caller::User)
            .await
            .expect("overview");
        let header = overview
            .headers
            .iter()
            .find(|h| h.id == id)
            .expect("still there");
        assert_eq!(header.group_id.as_deref(), Some("trash"));
        let secret = f
            .service
            .reveal(
                &Caller::User,
                id.clone(),
                holzi_lib::passwords::model::SecretField::Password,
            )
            .await
            .expect("data intact");
        assert_eq!(secret.value.as_str(), format!("{MARKER}-password-victim"));
        // A second delete from outside is not found and removes nothing for good.
        assert!(matches!(
            f.service
                .delete_item(&caller, &write_grant("s3"), id.clone())
                .await,
            Err(HolziError::PasswordsNotFound)
        ));
        assert!(
            f.service.get_item(&Caller::User, id).await.is_ok(),
            "still in the trash"
        );
    }
}

#[tokio::test]
async fn restoring_emptying_and_deleting_for_good_are_for_the_user_alone() {
    let f = fixture();
    let seeded = seed(&f).await;
    let all = [Grant::new(GrantAction::ReadWrite, Scope::All)];
    f.service
        .delete_item(&Caller::User, &[], seeded.s3.clone())
        .await
        .expect("user deletes");
    let mut everyone = outside_callers();
    everyone.push(Caller::BuiltinAgent);
    for caller in &everyone {
        for result in [
            f.service.restore(caller, vec![id_target(&seeded.s3)]).await,
            f.service
                .delete_permanently(caller, vec![id_target(&seeded.s3)], false)
                .await,
            f.service
                .trash_targets(caller, vec![id_target(&seeded.s3)])
                .await,
            f.service.empty_trash(caller, false).await,
        ] {
            assert!(
                matches!(result, Err(HolziError::PasswordsForbidden)),
                "{caller:?}: {result:?}"
            );
        }
        // The delete of an entry already in the trash stays "not there", also with a grant for all.
        let again = f.service.delete_item(caller, &all, seeded.s3.clone()).await;
        assert!(
            matches!(
                again,
                Err(HolziError::PasswordsNotFound) | Err(HolziError::PasswordsForbidden)
            ),
            "{caller:?}: {again:?}"
        );
    }
    // Nothing changed: the entry is still in the trash.
    assert!(f
        .service
        .get_item(&Caller::User, seeded.s3.clone())
        .await
        .is_ok());
    // The user restores it and later removes it for good in the trash.
    assert_eq!(
        f.service
            .restore(&Caller::User, vec![id_target(&seeded.s3)])
            .await
            .expect("restore"),
        1
    );
    f.service
        .delete_item(&Caller::User, &[], seeded.s3.clone())
        .await
        .expect("delete");
    assert_eq!(
        f.service
            .delete_permanently(&Caller::User, vec![id_target(&seeded.s3)], false)
            .await
            .expect("for good"),
        1
    );
    assert!(matches!(
        f.service.get_item(&Caller::User, seeded.s3).await,
        Err(HolziError::PasswordsNotFound)
    ));
}

#[tokio::test]
async fn a_read_grant_gives_the_totp_code_and_the_presets_but_nothing_outside_the_scope() {
    // Spec 049 FR-024, FR-027, FR-042: the code (never the secret) of an entry in the scope; an
    // entry without TOTP and one outside the scope answer the same.
    let f = fixture();
    let seeded = seed(&f).await;
    let without_totp = f
        .service
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some("no-otp".to_string()),
                tags: tags(&["s3"]),
                ..ItemInput::default()
            },
            None,
        )
        .await
        .expect("create");
    let grant = [Grant::new(GrantAction::Read, Scope::tags(["s3"]))];
    for caller in outside_callers() {
        let code = f
            .service
            .totp_code(&caller, &grant, seeded.s3.clone())
            .await
            .expect("a code in the scope");
        assert_eq!(code.code.len(), 6);
        assert!(code.remaining_seconds <= 30);
        for outside in [seeded.bank.clone(), without_totp.clone()] {
            assert!(matches!(
                f.service.totp_code(&caller, &grant, outside).await,
                Err(HolziError::PasswordsNotFound)
            ));
        }
        assert!(f.service.preset_list(&caller, &grant).await.is_ok());
    }
    assert!(matches!(
        f.service.preset_list(&Caller::BuiltinAgent, &grant).await,
        Err(HolziError::PasswordsForbidden)
    ));
}
