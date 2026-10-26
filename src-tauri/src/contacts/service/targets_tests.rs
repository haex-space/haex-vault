use super::*;
use crate::contacts::model::{ContactInput, TargetInput};
use crate::contacts::service::test_support::{account, add_phone, person, test_vault, TestVault};
use crate::error::HolziError;

async fn create(vault: &TestVault, given: &str, targets: Vec<TargetInput>) -> String {
    let input = ContactInput {
        targets,
        ..person(given, "Beck", vec![])
    };
    vault
        .service
        .create(&Caller::User, &[], input)
        .await
        .expect("create")
        .id
}

fn to(device: &str) -> TargetInput {
    TargetInput {
        device: device.to_owned(),
        account: None,
    }
}

async fn targets_of(vault: &TestVault, id: &str) -> Vec<(String, String, String)> {
    vault
        .service
        .get(&Caller::User, &[], id)
        .await
        .expect("get")
        .targets
        .into_iter()
        .map(|target| (target.device, target.account_type, target.account_name))
        .collect()
}

/// Inserts a link of a contact on a phone, as the phone's run would.
fn link(vault: &TestVault, device: &str, contact: &str, raw: i64) {
    let (device, contact) = (device.to_owned(), contact.to_owned());
    vault
        .db
        .write_blocking(move |tx| {
            tx.execute(
                "INSERT INTO haex_contacts_phone_links (id, vault_device_uuid, contact_id, \
                 raw_contact_id, lookup_key, account_type, account_name, version, read_only, base) \
                 VALUES (?1, ?2, ?3, ?4, '', 'com.google', 'mara@example.org', 1, 0, '{}')",
                params![
                    crate::contacts::ids::link_id(&device, raw),
                    device,
                    contact,
                    raw
                ],
            )
            .map(drop)
        })
        .expect("link");
}

fn links(vault: &TestVault, contact: &str) -> i64 {
    let contact = contact.to_owned();
    vault
        .db
        .read_blocking(move |q| {
            Ok(q.query_row(
                "SELECT COUNT(*) FROM haex_contacts_phone_links WHERE contact_id = ?1",
                params![contact],
                |r| r.get(0),
            )?
            .unwrap_or(0))
        })
        .expect("count links")
}

#[tokio::test]
async fn a_contact_made_in_holzi_has_no_target_unless_one_is_chosen() {
    let vault = test_vault();
    let phone = add_phone(
        &vault,
        "Pixel",
        true,
        vec![account("com.google", "mara@example.org")],
    );
    let only_holzi = create(&vault, "Anna", vec![]).await;
    assert!(targets_of(&vault, &only_holzi).await.is_empty());

    let to_phone = create(&vault, "Ben", vec![to(&phone)]).await;
    assert_eq!(
        targets_of(&vault, &to_phone).await,
        [(
            phone.clone(),
            "com.google".into(),
            "mara@example.org".into()
        )],
        "without an account the contact goes into the phone's target account"
    );
    let header = vault
        .service
        .list(&Caller::User, &[])
        .await
        .expect("list")
        .into_iter()
        .find(|header| header.id == to_phone)
        .expect("listed");
    assert_eq!(header.targets, [phone]);
}

#[tokio::test]
async fn only_phones_with_the_sync_on_are_offered() {
    let vault = test_vault();
    let on = add_phone(&vault, "Pixel", true, vec![account("", "")]);
    let off = add_phone(&vault, "Alt", false, vec![account("", "")]);
    let phones = vault.service.phones(&Caller::User).await.expect("phones");
    let offered: Vec<(&str, Option<&str>)> = phones
        .iter()
        .map(|phone| (phone.device.as_str(), phone.name.as_deref()))
        .collect();
    assert_eq!(offered, [(on.as_str(), Some("Pixel"))]);
    assert_eq!(phones[0].accounts, [account("", "")]);

    let refused = vault
        .service
        .create(
            &Caller::User,
            &[],
            ContactInput {
                targets: vec![to(&off)],
                ..person("Anna", "Beck", vec![])
            },
        )
        .await;
    assert!(matches!(refused, Err(HolziError::ContactsInvalid { .. })));
    assert!(
        vault
            .service
            .list(&Caller::User, &[])
            .await
            .expect("list")
            .iter()
            .all(|header| header.is_self),
        "a refused target writes nothing"
    );
}

#[tokio::test]
async fn targets_are_set_for_many_contacts_and_for_a_group() {
    let vault = test_vault();
    let phone = add_phone(
        &vault,
        "Pixel",
        true,
        vec![
            account("com.google", "mara@example.org"),
            account("com.example.work", "firma"),
        ],
    );
    let mut many = Vec::new();
    for n in 0..40 {
        many.push(create(&vault, &format!("Kontakt {n}"), vec![]).await);
    }
    vault
        .service
        .targets_set(
            &Caller::User,
            Selection::Contacts(many.clone()),
            phone.clone(),
            None,
        )
        .await
        .expect("set for 40");
    for id in &many {
        assert_eq!(targets_of(&vault, id).await.len(), 1);
    }

    let group = vault
        .service
        .group_save(&Caller::User, None, "Arbeit".into())
        .await
        .expect("group");
    let colleague = create(&vault, "Carla", vec![]).await;
    vault
        .service
        .group_members(
            &Caller::User,
            group.id.clone(),
            vec![colleague.clone()],
            vec![],
        )
        .await
        .expect("member");
    vault
        .service
        .targets_set(
            &Caller::User,
            Selection::Group(group.id),
            phone.clone(),
            Some(account("com.example.work", "firma")),
        )
        .await
        .expect("set for the group");
    assert_eq!(
        targets_of(&vault, &colleague).await,
        [(phone.clone(), "com.example.work".into(), "firma".into())]
    );

    let unknown = vault
        .service
        .targets_set(
            &Caller::User,
            Selection::Contacts(vec![colleague]),
            phone,
            Some(account("com.example.other", "x")),
        )
        .await;
    assert!(
        matches!(unknown, Err(HolziError::ContactsInvalid { .. })),
        "only an account the phone reported as writable"
    );
}

#[tokio::test]
async fn removing_a_target_deletes_on_the_phone_or_keeps_it_unsynced() {
    let vault = test_vault();
    let phone = add_phone(&vault, "Pixel", true, vec![account("", "")]);
    let delete = create(&vault, "Anna", vec![to(&phone)]).await;
    let keep = create(&vault, "Ben", vec![to(&phone)]).await;
    link(&vault, &phone, &delete, 1);
    link(&vault, &phone, &keep, 2);

    vault
        .service
        .targets_remove(
            &Caller::User,
            vec![delete.clone()],
            phone.clone(),
            OnPhone::Delete,
        )
        .await
        .expect("remove, delete on the phone");
    vault
        .service
        .targets_remove(&Caller::User, vec![keep.clone()], phone, OnPhone::Keep)
        .await
        .expect("remove, keep on the phone");
    assert!(targets_of(&vault, &delete).await.is_empty());
    assert!(targets_of(&vault, &keep).await.is_empty());
    assert_eq!(
        links(&vault, &delete),
        1,
        "the link stays, so the phone deletes the raw contact"
    );
    assert_eq!(links(&vault, &keep), 0, "the phone forgets the contact");
}

#[tokio::test]
async fn a_target_on_a_phone_switched_off_stays_but_no_new_one_is_made() {
    let vault = test_vault();
    let phone = add_phone(&vault, "Pixel", true, vec![account("", "")]);
    let anna = create(&vault, "Anna", vec![to(&phone)]).await;
    let device = phone.clone();
    vault
        .db
        .write_blocking(move |tx| {
            crate::contacts::phone::state::disable(tx, &device).map_err(haex_crdt::Error::from)
        })
        .expect("switch off");
    assert_eq!(targets_of(&vault, &anna).await.len(), 1);
    assert!(vault
        .service
        .phones(&Caller::User)
        .await
        .expect("phones")
        .is_empty());
    let ben = create(&vault, "Ben", vec![]).await;
    let refused = vault
        .service
        .targets_set(&Caller::User, Selection::Contacts(vec![ben]), phone, None)
        .await;
    assert!(matches!(refused, Err(HolziError::ContactsInvalid { .. })));
}

#[tokio::test]
async fn only_the_user_decides_where_a_contact_goes() {
    let vault = test_vault();
    let phone = add_phone(&vault, "Pixel", true, vec![account("", "")]);
    let agent = vault
        .service
        .create(
            &Caller::BuiltinAgent,
            &[crate::contacts::service::Grant::new(
                crate::contacts::access::GrantAction::ReadWrite,
                crate::contacts::access::Scope::All,
            )],
            ContactInput {
                targets: vec![to(&phone)],
                ..person("Anna", "Beck", vec![])
            },
        )
        .await;
    assert!(agent.is_err(), "an agent cannot choose a phone");
    assert!(vault.service.phones(&Caller::BuiltinAgent).await.is_err());
}

#[tokio::test]
async fn the_own_contact_goes_to_no_phone() {
    let vault = test_vault();
    let phone = add_phone(&vault, "Pixel", true, vec![account("", "")]);
    let anna = create(&vault, "Anna", vec![]).await;
    let own = crate::contacts::ids::SELF_CONTACT_ID.to_owned();

    let refused = vault
        .service
        .targets_set(
            &Caller::User,
            Selection::Contacts(vec![anna.clone(), own.clone()]),
            phone.clone(),
            None,
        )
        .await;
    assert!(matches!(refused, Err(HolziError::ContactsSelf)));
    assert!(
        targets_of(&vault, &anna).await.is_empty(),
        "a refused change writes nothing"
    );
    assert!(targets_of(&vault, &own).await.is_empty());
}

#[tokio::test]
async fn a_group_holding_the_own_contact_skips_it() {
    let vault = test_vault();
    let phone = add_phone(&vault, "Pixel", true, vec![account("", "")]);
    let group = vault
        .service
        .group_save(&Caller::User, None, "Familie".into())
        .await
        .expect("group");
    let input = ContactInput {
        groups: vec![group.id.clone()],
        ..person("Anna", "Beck", vec![])
    };
    let anna = vault
        .service
        .create(&Caller::User, &[], input)
        .await
        .expect("create")
        .id;
    vault
        .service
        .update(
            &Caller::User,
            &[],
            crate::contacts::ids::SELF_CONTACT_ID,
            crate::contacts::model::ContactPatch {
                groups: Some(vec![group.id.clone()]),
                ..Default::default()
            },
        )
        .await
        .expect("the own contact joins a group");

    vault
        .service
        .targets_set(&Caller::User, Selection::Group(group.id), phone, None)
        .await
        .expect("set");
    assert_eq!(targets_of(&vault, &anna).await.len(), 1);
    assert!(targets_of(&vault, crate::contacts::ids::SELF_CONTACT_ID)
        .await
        .is_empty());
}
