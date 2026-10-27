use super::*;
use crate::contacts::access::Caller;
use crate::contacts::model::{ContactPatch, ValueUpsert, ValuesPatch};
use crate::contacts::service::test_support::{card_vault, person, test_vault, value, TestVault};
use crate::contacts::share::envelope::{decode, Decoded, QR_MAX};
use crate::patch::Patch;
use crate::sync::test_support::Member;

/// The vault that reads a share: not the one that made it, so a card in it is not "its own".
const READER: [u8; 32] = [3; 32];

fn everything(options: &ContactShareOptions) -> ContactShareSelection {
    ContactShareSelection {
        values: options.rows.iter().map(|row| row.id.clone()).collect(),
        display_name: options.head.display_name.is_some(),
        nickname: options.head.nickname.is_some(),
        organization: options.head.organization.is_some(),
        job_title: options.head.job_title.is_some(),
        note: options.head.note.is_some(),
        holzi: match &options.holzi {
            ContactShareHolzi::None => ContactShareHolziChoice::None,
            ContactShareHolzi::Card { .. } => ContactShareHolziChoice::Card,
            ContactShareHolzi::Own {
                devices, relays, ..
            } => ContactShareHolziChoice::Own {
                devices: devices
                    .iter()
                    .map(|device| device.device_pubkey.clone())
                    .collect(),
                relays: relays.clone(),
            },
        },
    }
}

fn nothing() -> ContactShareSelection {
    ContactShareSelection {
        values: vec![],
        display_name: false,
        nickname: false,
        organization: false,
        job_title: false,
        note: false,
        holzi: ContactShareHolziChoice::None,
    }
}

async fn options_of(vault: &TestVault, id: &str) -> ContactShareOptions {
    vault
        .service
        .share_options(&Caller::User, id.to_owned())
        .await
        .expect("options")
}

async fn share(
    vault: &TestVault,
    id: &str,
    selection: ContactShareSelection,
) -> Result<ContactShareOutput> {
    vault
        .service
        .share_build(&Caller::User, id.to_owned(), selection)
        .await
}

fn read(output: &ContactShareOutput) -> Decoded {
    decode(&output.link, &READER).expect("a valid share")
}

/// Anna with phone, e-mail, birthday, organization and note.
async fn anna(vault: &TestVault) -> String {
    let mut input = person(
        "Anna",
        "Beck",
        vec![
            value("phone", "mobile", "0170 1234567"),
            value("email", "home", "anna@example.org"),
            value("event", "birthday", r#"{"date":"1990-05-17"}"#),
        ],
    );
    input.head.organization = Some("Beck GmbH".to_owned());
    input.head.note = Some("Kennengelernt auf der Messe".to_owned());
    vault
        .service
        .create(&Caller::User, &[], input)
        .await
        .expect("create")
        .id
}

#[tokio::test]
async fn everything_unselected_is_absent_from_the_share() {
    let vault = test_vault();
    let id = anna(&vault).await;
    let options = options_of(&vault, &id).await;
    assert_eq!(options.rows.len(), 3);
    assert!(options.head.organization.is_some() && options.head.note.is_some());

    let email = options
        .rows
        .iter()
        .find(|row| row.kind == "email")
        .expect("email")
        .id
        .clone();
    let read = read(
        &share(
            &vault,
            &id,
            ContactShareSelection {
                values: vec![email],
                ..nothing()
            },
        )
        .await
        .expect("share"),
    );
    assert_eq!(read.envelope.name, "Anna Beck");
    assert_eq!(
        read.envelope
            .values
            .iter()
            .map(|v| v.value.as_str())
            .collect::<Vec<_>>(),
        ["anna@example.org"]
    );
    assert_eq!(read.envelope.head, Default::default());
    assert!(read.envelope.card.is_none());

    let all = self::read(
        &share(&vault, &id, everything(&options))
            .await
            .expect("share"),
    );
    assert_eq!(all.envelope.values.len(), 3);
    assert_eq!(all.envelope.head.organization.as_deref(), Some("Beck GmbH"));
    assert_eq!(
        all.envelope.head.note.as_deref(),
        Some("Kennengelernt auf der Messe")
    );
}

#[tokio::test]
async fn the_order_of_the_contact_is_kept_whatever_the_order_of_the_choice() {
    let vault = test_vault();
    let id = anna(&vault).await;
    let options = options_of(&vault, &id).await;
    let mut selection = everything(&options);
    selection.values.reverse();
    let read = read(&share(&vault, &id, selection).await.expect("share"));
    let wanted: Vec<_> = options.rows.iter().map(|row| row.value.clone()).collect();
    let got: Vec<_> = read
        .envelope
        .values
        .iter()
        .map(|v| v.value.clone())
        .collect();
    assert_eq!(got, wanted);
}

#[tokio::test]
async fn the_own_card_carries_only_the_chosen_devices_and_relays() {
    let vault = card_vault();
    let own = crate::contacts::ids::SELF_CONTACT_ID;
    let options = options_of(&vault, own).await;
    let ContactShareHolzi::Own {
        can_sign,
        devices,
        relays,
        ..
    } = &options.holzi
    else {
        panic!("the own contact offers the own addresses");
    };
    assert!(*can_sign);
    assert_eq!(devices.len(), 1);
    assert!(!relays.is_empty());

    let with_all = read(
        &share(&vault, own, everything(&options))
            .await
            .expect("share"),
    );
    let card = with_all.card.expect("a card").card;
    assert_eq!(card.devices.len(), 1);
    assert_eq!(card.nostr_relays, *relays);
    assert!(card.values.is_empty());

    let identity_only = ContactShareSelection {
        holzi: ContactShareHolziChoice::Own {
            devices: vec![],
            relays: vec![],
        },
        ..nothing()
    };
    let card = read(&share(&vault, own, identity_only).await.expect("share"))
        .card
        .expect("the identity goes along")
        .card;
    assert!(card.devices.is_empty() && card.nostr_relays.is_empty());

    let one_relay = ContactShareSelection {
        holzi: ContactShareHolziChoice::Own {
            devices: vec![],
            relays: vec![relays[0].clone()],
        },
        ..nothing()
    };
    let card = read(&share(&vault, own, one_relay).await.expect("share"))
        .card
        .expect("a card")
        .card;
    assert_eq!(card.nostr_relays, [relays[0].clone()]);
}

#[tokio::test]
async fn the_own_contact_without_a_name_shares_the_npub_as_its_name() {
    let vault = card_vault();
    let own = crate::contacts::ids::SELF_CONTACT_ID;
    let options = options_of(&vault, own).await;
    assert!(options.name.starts_with("npub1"), "{}", options.name);
    assert!(options.can_share);
    let read = read(
        &share(&vault, own, everything(&options))
            .await
            .expect("share"),
    );
    assert_eq!(read.envelope.name, options.name);

    vault
        .service
        .update(
            &Caller::User,
            &[],
            own,
            ContactPatch {
                given_name: Patch::Set("Martin".to_owned()),
                ..ContactPatch::default()
            },
        )
        .await
        .expect("name");
    assert_eq!(options_of(&vault, own).await.name, "Martin");
}

#[tokio::test]
async fn the_own_holzi_values_are_never_rows() {
    let vault = card_vault();
    let own = crate::contacts::ids::SELF_CONTACT_ID;
    vault
        .service
        .update(
            &Caller::User,
            &[],
            own,
            ContactPatch {
                values: Some(ValuesPatch {
                    upsert: vec![
                        ValueUpsert {
                            id: None,
                            kind: "holzi_nostr_relay".to_owned(),
                            label: "other".to_owned(),
                            value: "wss://typed.example.org".to_owned(),
                            position: None,
                        },
                        ValueUpsert {
                            id: None,
                            kind: "email".to_owned(),
                            label: "home".to_owned(),
                            value: "me@example.org".to_owned(),
                            position: None,
                        },
                    ],
                    remove: vec![],
                }),
                ..ContactPatch::default()
            },
        )
        .await
        .expect("values");
    let rows = options_of(&vault, own).await.rows;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, "email");
}

#[test]
fn a_linked_device_shares_without_a_card_and_says_so() {
    let main = Member::genesis();
    let linked = Member::join(&main);
    let own = crate::contacts::ids::SELF_CONTACT_ID;
    let options = crate::storage::query::read(linked.device.db(), |r| {
        options(r, own).map_err(haex_crdt::Error::from)
    })
    .expect("options");
    let ContactShareHolzi::Own {
        can_sign, devices, ..
    } = &options.holzi
    else {
        panic!("the own addresses are offered");
    };
    assert!(!can_sign);
    assert!(devices.is_empty());

    let selection = ContactShareSelection {
        holzi: ContactShareHolziChoice::Own {
            devices: vec![],
            relays: vec![],
        },
        ..nothing()
    };
    let output = crate::storage::query::read(linked.device.db(), |r| {
        build(r, own, &selection).map_err(haex_crdt::Error::from)
    })
    .expect("a share without a card");
    assert!(decode(&output.link, &READER).expect("valid").card.is_none());
}

#[tokio::test]
async fn nothing_to_share_is_refused_and_options_say_so() {
    let vault = test_vault();
    // No vault identity yet, no name, no details: the own contact has nothing to give.
    let own = crate::contacts::ids::SELF_CONTACT_ID;
    let options = options_of(&vault, own).await;
    assert!(!options.can_share);
    assert!(matches!(
        share(&vault, own, everything(&options)).await,
        Err(HolziError::ShareInvalid { .. })
    ));

    // A contact known only by a number has no name; with nothing chosen there is nothing.
    let id = vault
        .service
        .create(
            &Caller::User,
            &[],
            crate::contacts::model::ContactInput {
                values: vec![value("phone", "mobile", "0170 1234567")],
                ..Default::default()
            },
        )
        .await
        .expect("create")
        .id;
    assert!(options_of(&vault, &id).await.can_share);
    assert!(matches!(
        share(&vault, &id, nothing()).await,
        Err(HolziError::ShareInvalid { .. })
    ));
}

#[tokio::test]
async fn a_value_of_another_contact_or_a_foreign_device_is_refused() {
    let vault = card_vault();
    let mine = anna(&vault).await;
    let other = vault
        .service
        .create(
            &Caller::User,
            &[],
            person(
                "Bodo",
                "Frei",
                vec![value("phone", "mobile", "030 5550101")],
            ),
        )
        .await
        .expect("create");
    let foreign = ContactShareSelection {
        values: vec![other.values[0].id.clone()],
        ..nothing()
    };
    assert!(matches!(
        share(&vault, &mine, foreign).await,
        Err(HolziError::ShareInvalid { .. })
    ));

    let own = crate::contacts::ids::SELF_CONTACT_ID;
    let stranger = ContactShareSelection {
        holzi: ContactShareHolziChoice::Own {
            devices: vec![hex(&[9; 32])],
            relays: vec![],
        },
        ..nothing()
    };
    assert!(matches!(
        share(&vault, own, stranger).await,
        Err(HolziError::ShareInvalid { .. })
    ));
    let not_hex = ContactShareSelection {
        holzi: ContactShareHolziChoice::Own {
            devices: vec!["zz".to_owned()],
            relays: vec![],
        },
        ..nothing()
    };
    assert!(share(&vault, own, not_hex).await.is_err());
    let card_for_a_contact_without_one = ContactShareSelection {
        holzi: ContactShareHolziChoice::Card,
        ..nothing()
    };
    assert!(matches!(
        share(&vault, &mine, card_for_a_contact_without_one).await,
        Err(HolziError::ShareInvalid { .. })
    ));
}

#[tokio::test]
async fn a_share_over_the_limit_is_too_large_and_the_qr_code_only_fits_a_short_one() {
    let vault = test_vault();
    let small = anna(&vault).await;
    let options = options_of(&vault, &small).await;
    let output = share(&vault, &small, everything(&options))
        .await
        .expect("share");
    assert!(output.link.len() <= QR_MAX);
    assert!(output
        .qr_svg
        .as_deref()
        .is_some_and(|svg| svg.contains("<svg")));
    assert_eq!(
        usize::try_from(output.size).unwrap(),
        output.link.chars().count()
    );

    let big = vault
        .service
        .create(
            &Caller::User,
            &[],
            person(
                "Bert",
                "Groß",
                (0..4)
                    .map(|n| value("custom", "custom:Notiz", &format!("{n}{}", "x".repeat(900))))
                    .collect(),
            ),
        )
        .await
        .expect("create")
        .id;
    let options = options_of(&vault, &big).await;
    let output = share(&vault, &big, everything(&options))
        .await
        .expect("share");
    assert!(output.link.len() > QR_MAX);
    assert!(output.qr_svg.is_none(), "too large for a QR code");
    assert_eq!(read(&output).envelope.values.len(), 4);

    let huge = vault
        .service
        .create(
            &Caller::User,
            &[],
            person(
                "Hugo",
                "Riese",
                (0..60)
                    .map(|n| {
                        value(
                            "custom",
                            "custom:Notiz",
                            &format!("{n}{}", "y".repeat(4_000)),
                        )
                    })
                    .collect(),
            ),
        )
        .await
        .expect("create")
        .id;
    let options = options_of(&vault, &huge).await;
    assert!(matches!(
        share(&vault, &huge, everything(&options)).await,
        Err(HolziError::ShareTooLarge)
    ));
}

// --- Another contact's confirmed addresses (spec 060 US3) ---

/// Ben's card text from a vault of his own, and his vault key.
async fn bens_card() -> (TestVault, String) {
    let ben = card_vault();
    ben.db
        .write_blocking(|tx| own_card::refresh(tx).map_err(haex_crdt::Error::from))
        .expect("refresh");
    let text = ben
        .db
        .read_blocking(|q| own_card::stored_text(q).map_err(haex_crdt::Error::from))
        .expect("read")
        .expect("a card");
    (ben, text)
}

#[tokio::test]
async fn a_foreign_contact_shares_its_original_card_unchanged_and_the_row_can_be_deselected() {
    let (_ben, text) = bens_card().await;
    let mara = card_vault();
    let stored = format::parse(&text).expect("a card");
    let contact = mara
        .service
        .accept_defaults(text, None)
        .await
        .expect("accept");
    let options = options_of(&mara, &contact.id).await;
    let ContactShareHolzi::Card {
        npub,
        devices,
        relays,
        ..
    } = &options.holzi
    else {
        panic!("a contact with a stored card offers one row");
    };
    assert_eq!(*npub, received::npub(&stored.card.vault));
    assert_eq!(
        (*devices, *relays),
        (1, u32::try_from(stored.card.nostr_relays.len()).unwrap())
    );

    let with = read(
        &share(&mara, &contact.id, everything(&options))
            .await
            .expect("share"),
    );
    assert_eq!(
        with.envelope.card.as_deref(),
        Some(stored.bytes.as_slice()),
        "the card is the stored one, byte for byte"
    );
    assert_eq!(with.card.expect("valid").card.vault, stored.card.vault);

    let mut without = everything(&options);
    without.holzi = ContactShareHolziChoice::None;
    let without = read(&share(&mara, &contact.id, without).await.expect("share"));
    assert!(without.envelope.card.is_none());
}

#[tokio::test]
async fn a_contact_without_addresses_shares_like_any_other_without_an_error() {
    let vault = test_vault();
    let id = anna(&vault).await;
    let options = options_of(&vault, &id).await;
    assert_eq!(options.holzi, ContactShareHolzi::None);
    let read = read(
        &share(&vault, &id, everything(&options))
            .await
            .expect("share"),
    );
    assert!(read.envelope.card.is_none());
}

#[tokio::test]
async fn a_nameless_contact_with_a_card_gets_the_npub_as_its_name() {
    let (_ben, text) = bens_card().await;
    let vault = card_vault();
    let stored = format::parse(&text).expect("a card");
    let id = vault
        .service
        .create(
            &Caller::User,
            &[],
            crate::contacts::model::ContactInput {
                values: vec![value("phone", "mobile", "0170 1234567")],
                ..Default::default()
            },
        )
        .await
        .expect("create")
        .id;
    vault
        .service
        .accept_defaults(text, Some(id.clone()))
        .await
        .expect("accept");
    let options = options_of(&vault, &id).await;
    assert_eq!(options.name, received::npub(&stored.card.vault));
    let read = read(
        &share(&vault, &id, everything(&options))
            .await
            .expect("share"),
    );
    assert_eq!(read.envelope.name, options.name);
}

#[tokio::test]
async fn values_of_an_unknown_kind_travel_unchanged_and_a_hand_typed_holzi_address_is_a_row() {
    let vault = test_vault();
    let id = anna(&vault).await;
    let there = id.clone();
    vault
        .db
        .write_blocking(move |tx| {
            crate::contacts::store::insert_value(tx, &there, "telepathy", "weird", "think", 9)?;
            crate::contacts::store::insert_value(
                tx,
                &there,
                "holzi_nostr_relay",
                "other",
                "wss://typed.example.org",
                10,
            )?;
            Ok(())
        })
        .expect("values");
    let options = options_of(&vault, &id).await;
    assert!(options.rows.iter().any(|row| row.kind == "telepathy"));
    let read = read(
        &share(&vault, &id, everything(&options))
            .await
            .expect("share"),
    );
    let values: Vec<_> = read
        .envelope
        .values
        .iter()
        .map(|v| (v.kind.as_str(), v.value.as_str()))
        .collect();
    assert!(values.contains(&("telepathy", "think")));
    assert!(values.contains(&("holzi_nostr_relay", "wss://typed.example.org")));
    assert!(read.card.is_none(), "typed addresses confirm nothing");
}

#[tokio::test]
async fn a_stored_card_that_fails_its_check_is_not_offered() {
    let (_ben, text) = bens_card().await;
    let mara = card_vault();
    let contact = mara
        .service
        .accept_defaults(text, None)
        .await
        .expect("accept");
    mara.db
        .write_blocking(|tx| {
            tx.execute("UPDATE haex_contacts_cards SET vault_pubkey = 'ab'", &[])?;
            Ok(())
        })
        .expect("break the row");
    assert_eq!(
        options_of(&mara, &contact.id).await.holzi,
        ContactShareHolzi::None
    );
}
