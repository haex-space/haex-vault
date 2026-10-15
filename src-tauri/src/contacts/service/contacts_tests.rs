use haex_crdt::rusqlite::params;

use super::*;
use crate::contacts::access::{GrantAction, Scope};
use crate::contacts::model::{ContactPatch, OriginKind, ValueUpsert, ValuesPatch};
use crate::contacts::service::test_support::{person, test_vault, value};
use crate::error::HolziError;
use crate::patch::Patch;
use crate::storage::query::Query;

fn anna() -> crate::contacts::model::ContactInput {
    person(
        "Anna",
        "Beck",
        vec![
            value("phone", "mobile", "0170 1234567"),
            value("phone", "home", "030 5550101"),
            value("email", "home", "anna@example.org"),
            value("custom", "custom:Kundennummer", "4711"),
        ],
    )
}

/// The HLC column of a value row, to see whether a write touched it.
fn value_hlc(vault: &crate::contacts::service::test_support::TestVault, value_id: &str) -> String {
    let id = value_id.to_owned();
    vault
        .db
        .read_blocking(move |q| {
            q.query_row(
                "SELECT quote(haex_hlc_no_sync) FROM haex_contacts_values WHERE id = ?1",
                params![id],
                |r| r.get::<_, String>(0),
            )
        })
        .expect("read")
        .expect("the row exists")
}

#[tokio::test]
async fn a_new_contact_keeps_its_values_and_origin() {
    let vault = test_vault();
    let contact = vault
        .service
        .create(&Caller::User, &[], anna())
        .await
        .expect("create");
    assert_eq!(contact.name, "Anna Beck");
    assert_eq!(contact.values.len(), 4);
    assert!(contact.values.iter().all(|v| v.known));
    assert_eq!(contact.origin.kind, OriginKind::Holzi);
    assert_eq!(contact.origin.caller, "user");
    assert_eq!(contact.origin.device, vault.db.device_id().to_string());

    let again = vault
        .service
        .get(&Caller::User, &[], &contact.id)
        .await
        .expect("get");
    assert_eq!(again, contact);
    let headers = vault.service.list(&Caller::User, &[]).await.expect("list");
    assert_eq!(headers.len(), 1);
    assert_eq!(headers[0].phones, ["0170 1234567", "030 5550101"]);
    assert_eq!(headers[0].emails, ["anna@example.org"]);
}

#[tokio::test]
async fn a_contact_without_anything_to_find_it_by_is_refused() {
    let vault = test_vault();
    let mut empty = person(" ", "", vec![value("custom", "custom:Notiz", "x")]);
    empty.head.given_name = None;
    let refused = vault.service.create(&Caller::User, &[], empty).await;
    assert!(matches!(refused, Err(HolziError::ContactsInvalid { .. })));
}

#[tokio::test]
async fn invalid_values_and_long_notes_are_refused() {
    let vault = test_vault();
    let bad = person("A", "B", vec![value("email", "home", "no-at-sign")]);
    assert!(matches!(
        vault.service.create(&Caller::User, &[], bad).await,
        Err(HolziError::ContactsInvalid { .. })
    ));
    let mut long = person("A", "B", vec![]);
    long.head.note = Some("x".repeat(64 * 1024 + 1));
    assert!(matches!(
        vault.service.create(&Caller::User, &[], long).await,
        Err(HolziError::ContactsInvalid { .. })
    ));
}

#[tokio::test]
async fn an_update_touches_only_the_rows_it_names() {
    let vault = test_vault();
    let contact = vault
        .service
        .create(&Caller::User, &[], anna())
        .await
        .expect("create");
    let mobile = contact
        .values
        .iter()
        .find(|v| v.label == "mobile")
        .expect("mobile")
        .clone();
    let home = contact
        .values
        .iter()
        .find(|v| v.label == "home" && v.kind == "phone")
        .expect("home")
        .clone();
    let home_before = value_hlc(&vault, &home.id);

    let patch = ContactPatch {
        values: Some(ValuesPatch {
            upsert: vec![ValueUpsert {
                id: Some(mobile.id.clone()),
                kind: "phone".into(),
                label: "mobile".into(),
                value: "0171 7654321".into(),
                position: None,
            }],
            remove: vec![],
        }),
        ..ContactPatch::default()
    };
    let updated = vault
        .service
        .update(&Caller::User, &[], &contact.id, patch)
        .await
        .expect("update");
    let changed = updated
        .values
        .iter()
        .find(|v| v.id == mobile.id)
        .expect("same row");
    assert_eq!(changed.value, "0171 7654321");
    assert_eq!(value_hlc(&vault, &home.id), home_before, "untouched row");
    assert_eq!(updated.values.len(), 4);
}

#[tokio::test]
async fn head_fields_are_set_cleared_or_kept() {
    let vault = test_vault();
    let mut input = anna();
    input.head.organization = Some("Gärtnerei".into());
    let contact = vault
        .service
        .create(&Caller::User, &[], input)
        .await
        .expect("create");
    let patch = ContactPatch {
        nickname: Patch::Set("Anni".into()),
        organization: Patch::Clear,
        ..ContactPatch::default()
    };
    let updated = vault
        .service
        .update(&Caller::User, &[], &contact.id, patch)
        .await
        .expect("update");
    assert_eq!(updated.head.nickname.as_deref(), Some("Anni"));
    assert_eq!(updated.head.organization, None);
    assert_eq!(updated.head.given_name.as_deref(), Some("Anna"));
}

#[tokio::test]
async fn values_of_unknown_kinds_survive_and_stay_read_only() {
    let vault = test_vault();
    let contact = vault
        .service
        .create(&Caller::User, &[], anna())
        .await
        .expect("create");
    let id = contact.id.clone();
    vault
        .db
        .write(move |tx| {
            tx.execute(
                "INSERT INTO haex_contacts_values (id, contact_id, kind, label, value, position) \
                 VALUES ('future', ?1, 'future_kind', 'other', 'something new', 0)",
                params![id],
            )?;
            Ok(())
        })
        .await
        .expect("a value of a newer version");

    let patch = ContactPatch {
        nickname: Patch::Set("Anni".into()),
        ..ContactPatch::default()
    };
    let updated = vault
        .service
        .update(&Caller::User, &[], &contact.id, patch)
        .await
        .expect("update");
    let future = updated
        .values
        .iter()
        .find(|v| v.id == "future")
        .expect("still there");
    assert!(!future.known);
    assert_eq!(future.value, "something new");

    let change = ContactPatch {
        values: Some(ValuesPatch {
            upsert: vec![ValueUpsert {
                id: Some("future".into()),
                kind: "future_kind".into(),
                label: "other".into(),
                value: "changed".into(),
                position: None,
            }],
            remove: vec![],
        }),
        ..ContactPatch::default()
    };
    // The kind is unknown, so the value check already refuses it.
    assert!(vault
        .service
        .update(&Caller::User, &[], &contact.id, change)
        .await
        .is_err());
    let remove = ContactPatch {
        values: Some(ValuesPatch {
            upsert: vec![],
            remove: vec!["future".into()],
        }),
        ..ContactPatch::default()
    };
    assert!(matches!(
        vault
            .service
            .update(&Caller::User, &[], &contact.id, remove)
            .await,
        Err(HolziError::ContactsUnknownKindReadOnly)
    ));
}

#[tokio::test]
async fn a_value_of_another_contact_is_not_found() {
    let vault = test_vault();
    let anna = vault
        .service
        .create(&Caller::User, &[], anna())
        .await
        .expect("create");
    let ben = vault
        .service
        .create(&Caller::User, &[], person("Ben", "Krüger", vec![]))
        .await
        .expect("create");
    let patch = ContactPatch {
        values: Some(ValuesPatch {
            upsert: vec![],
            remove: vec![anna.values[0].id.clone()],
        }),
        ..ContactPatch::default()
    };
    assert!(matches!(
        vault
            .service
            .update(&Caller::User, &[], &ben.id, patch)
            .await,
        Err(HolziError::ContactsNotFound)
    ));
}

#[tokio::test]
async fn removing_the_last_name_and_number_is_refused_and_writes_nothing() {
    let vault = test_vault();
    let contact = vault
        .service
        .create(
            &Caller::User,
            &[],
            person("Anna", "", vec![value("custom", "custom:x", "1")]),
        )
        .await
        .expect("create");
    let patch = ContactPatch {
        given_name: Patch::Clear,
        family_name: Patch::Clear,
        ..ContactPatch::default()
    };
    assert!(vault
        .service
        .update(&Caller::User, &[], &contact.id, patch)
        .await
        .is_err());
    let still = vault
        .service
        .get(&Caller::User, &[], &contact.id)
        .await
        .expect("get");
    assert_eq!(still.head.given_name.as_deref(), Some("Anna"));
}

#[tokio::test]
async fn deleting_removes_the_contact_and_its_values() {
    let vault = test_vault();
    let contact = vault
        .service
        .create(&Caller::User, &[], anna())
        .await
        .expect("create");
    vault
        .service
        .delete(&Caller::User, &[], vec![contact.id.clone()])
        .await
        .expect("delete");
    assert!(matches!(
        vault.service.get(&Caller::User, &[], &contact.id).await,
        Err(HolziError::ContactsNotFound)
    ));
    let left = vault
        .db
        .read_blocking(|q| {
            q.query_row("SELECT COUNT(*) FROM haex_contacts_values", &[], |r| {
                r.get::<_, i64>(0)
            })
        })
        .expect("count");
    assert_eq!(left, Some(0));
}

#[tokio::test]
async fn other_callers_see_only_their_scope_and_cannot_tell_the_rest_apart() {
    let vault = test_vault();
    let work = vault
        .service
        .group_save(&Caller::User, None, "Arbeit".into())
        .await
        .expect("group");
    let mut colleague = person("Carla", "Dorn", vec![]);
    colleague.groups = vec![work.id.clone()];
    let colleague = vault
        .service
        .create(&Caller::User, &[], colleague)
        .await
        .expect("create");
    let private = vault
        .service
        .create(&Caller::User, &[], anna())
        .await
        .expect("create");

    let extension = Caller::Extension { id: "mail".into() };
    assert!(matches!(
        vault.service.list(&extension, &[]).await,
        Err(HolziError::ContactsForbidden)
    ));
    let grants = [Grant::new(
        GrantAction::Read,
        Scope::Groups([work.id.clone()].into()),
    )];
    let listed = vault.service.list(&extension, &grants).await.expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, colleague.id);
    // Outside the scope looks exactly like missing (FR-031).
    assert!(matches!(
        vault.service.get(&extension, &grants, &private.id).await,
        Err(HolziError::ContactsNotFound)
    ));
    assert!(matches!(
        vault.service.get(&extension, &grants, "no-such-id").await,
        Err(HolziError::ContactsNotFound)
    ));
    // A read grant never writes.
    assert!(matches!(
        vault
            .service
            .delete(&extension, &grants, vec![colleague.id.clone()])
            .await,
        Err(HolziError::ContactsForbidden)
    ));
}

#[tokio::test]
async fn an_agent_with_a_write_grant_creates_with_its_own_origin() {
    let vault = test_vault();
    let grants = [Grant::new(GrantAction::ReadWrite, Scope::All)];
    let contact = vault
        .service
        .create(&Caller::BuiltinAgent, &grants, anna())
        .await
        .expect("create");
    assert_eq!(contact.origin.caller, "builtin_agent");
    assert!(matches!(
        vault
            .service
            .create(&Caller::BuiltinAgent, &[], anna())
            .await,
        Err(HolziError::ContactsForbidden)
    ));
}

#[tokio::test]
async fn search_finds_only_within_the_scope() {
    let vault = test_vault();
    let work = vault
        .service
        .group_save(&Caller::User, None, "Arbeit".into())
        .await
        .expect("group");
    let mut colleague = person("Anna", "Dorn", vec![]);
    colleague.groups = vec![work.id.clone()];
    let colleague = vault
        .service
        .create(&Caller::User, &[], colleague)
        .await
        .expect("create");
    vault
        .service
        .create(&Caller::User, &[], anna())
        .await
        .expect("create");

    let found = vault
        .service
        .search(&Caller::User, &[], "anna")
        .await
        .expect("search");
    assert_eq!(found.len(), 2);
    let extension = Caller::Extension { id: "mail".into() };
    let grants = [Grant::new(
        GrantAction::Read,
        Scope::Groups([work.id.clone()].into()),
    )];
    let found = vault
        .service
        .search(&extension, &grants, "anna")
        .await
        .expect("search");
    assert_eq!(
        found.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(),
        [colleague.id.as_str()]
    );
    assert!(matches!(
        vault.service.search(&extension, &[], "anna").await,
        Err(HolziError::ContactsForbidden)
    ));
}
