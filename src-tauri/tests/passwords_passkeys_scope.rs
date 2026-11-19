//! Which passkeys a caller reaches through the passkey service (spec 036, US5, T060,
//! `contracts/passkey-service.md`): one passkey reached on two ways is one candidate, two need a
//! choice; the scope of the grants decides what exists (trash and passkeys without an entry never);
//! a link counts only when its target and the passkey's own entry are readable.

// These tests read raw vault state that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

#[path = "common/passkey_fixture.rs"]
mod passkey_fixture;

use haex_crdt::rusqlite::params;

use holzi_lib::passwords::access::{Caller, Grant, GrantAction, Scope};
use holzi_lib::passwords::model_passkeys::PasskeyListRequest;
use holzi_lib::HaexError;

use passkey_fixture::{confirm_request, create_request, extension, fixture, RP};

#[tokio::test]
async fn two_passkeys_need_a_choice_but_one_reached_twice_is_one_candidate() {
    let fx = fixture();
    let source = fx.entry("Konto", &[]).await;
    let target = fx.entry("Kopie", &[]).await;
    let first = fx.create(&source, &[-7]).await;
    let passkey_id = fx.passkey_id();
    fx.link(&target, &passkey_id);
    let one = fx
        .service
        .passkey_confirm(&Caller::User, &[], confirm_request(&[]))
        .await
        .expect("one candidate");
    assert_eq!(one.item_id, source, "its own entry comes first");

    let mut through_target = confirm_request(&[]);
    through_target.item_id = Some(target.clone());
    let linked = fx
        .service
        .passkey_confirm(&Caller::User, &[], through_target)
        .await
        .expect("through the link");
    assert_eq!(linked.item_id, target);
    assert_eq!(linked.credential_id, first.credential_id);

    let second = fx.create(&target, &[-8]).await;
    let choice = fx
        .service
        .passkey_confirm(&Caller::User, &[], confirm_request(&[]))
        .await;
    let Err(HaexError::PasswordsPasskeyChoiceRequired { candidates }) = choice else {
        panic!("two passkeys need a choice: {choice:?}");
    };
    assert_eq!(candidates.len(), 2);
    let picked = fx
        .service
        .passkey_confirm(
            &Caller::User,
            &[],
            confirm_request(&[&second.credential_id]),
        )
        .await
        .expect("picked");
    assert_eq!(picked.credential_id, second.credential_id);
}

#[tokio::test]
async fn the_scope_decides_which_passkeys_exist_for_a_caller() {
    let fx = fixture();
    let a = fx.entry("A", &["a"]).await;
    let b = fx.entry("B", &["b"]).await;
    let trashed = fx.entry("Weg", &["a"]).await;
    let pk_a = fx.create(&a, &[-7]).await;
    fx.create(&b, &[-7]).await;
    fx.create(&trashed, &[-7]).await;
    fx.service
        .delete_item(&Caller::User, &[], trashed.clone())
        .await
        .expect("trash");
    fx.db
        .with_connection(|c| {
            c.execute(
                "INSERT INTO haex_passwords_passkeys \
                 (id, item_id, credential_id, relying_party_id, user_handle, private_key, \
                  public_key) VALUES ('orphan', NULL, 'b3JwaGFu', ?1, 'dQ', 'k', 'p')",
                params![RP],
            )?;
            Ok(())
        })
        .expect("orphan");

    for caller in [
        Caller::Extension {
            id: "ext".to_string(),
        },
        Caller::ExternalAgent {
            id: "agent".to_string(),
        },
    ] {
        let grants = [Grant::new(GrantAction::Read, Scope::tags(["a"]))];
        let listed = fx
            .service
            .passkey_list(&caller, &grants, PasskeyListRequest::default())
            .await
            .expect("list");
        assert_eq!(listed.len(), 1, "only the passkey at the entry with tag a");
        assert_eq!(listed[0].item_id, a);
        let confirmed = fx
            .service
            .passkey_confirm(&caller, &grants, confirm_request(&[]))
            .await
            .expect("confirm");
        assert_eq!(confirmed.credential_id, pk_a.credential_id);
    }

    let none = [Grant::new(GrantAction::Read, Scope::tags(["c"]))];
    let (ext, _) = extension("c", GrantAction::Read);
    let missing = fx
        .service
        .passkey_confirm(&ext, &none, confirm_request(&[]))
        .await;
    assert!(matches!(missing, Err(HaexError::PasswordsNotFound)));
    let no_grant = fx
        .service
        .passkey_list(&ext, &[], PasskeyListRequest::default())
        .await;
    assert!(matches!(no_grant, Err(HaexError::PasswordsForbidden)));
    let agent = fx
        .service
        .passkey_list(&Caller::BuiltinAgent, &[], PasskeyListRequest::default())
        .await;
    assert!(matches!(agent, Err(HaexError::PasswordsForbidden)));

    let (writer, grants) = extension("a", GrantAction::ReadWrite);
    let outside = fx
        .service
        .passkey_create(&writer, &grants, create_request(&b, &[-7]))
        .await;
    assert!(matches!(outside, Err(HaexError::PasswordsNotFound)));
    let (reader, grants) = extension("a", GrantAction::Read);
    let read_only = fx
        .service
        .passkey_create(&reader, &grants, create_request(&a, &[-7]))
        .await;
    assert!(matches!(read_only, Err(HaexError::PasswordsForbidden)));
}

#[tokio::test]
async fn a_link_counts_only_when_target_and_source_are_readable() {
    let fx = fixture();
    let source = fx.entry("Quelle", &["b"]).await;
    let target = fx.entry("Ziel", &["a"]).await;
    fx.create(&source, &[-7]).await;
    let passkey_id = fx.passkey_id();
    fx.link(&target, &passkey_id);

    let (only_a, grants) = extension("a", GrantAction::Read);
    let hidden = fx
        .service
        .passkey_confirm(&only_a, &grants, confirm_request(&[]))
        .await;
    assert!(matches!(hidden, Err(HaexError::PasswordsNotFound)));

    let both = [Grant::new(GrantAction::Read, Scope::tags(["a", "b"]))];
    let mut through_target = confirm_request(&[]);
    through_target.item_id = Some(target.clone());
    let confirmed = fx
        .service
        .passkey_confirm(&only_a, &both, through_target)
        .await
        .expect("both readable");
    assert_eq!(confirmed.item_id, target);
    let listed = fx
        .service
        .passkey_list(
            &only_a,
            &both,
            PasskeyListRequest {
                item_id: Some(target.clone()),
                ..PasskeyListRequest::default()
            },
        )
        .await
        .expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(
        listed[0].linked_from_item_id.as_deref(),
        Some(source.as_str())
    );
}
