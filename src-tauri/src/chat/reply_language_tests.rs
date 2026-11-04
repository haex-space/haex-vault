use super::*;
use crate::passwords::test_support::open_test_vault;
use crate::storage::query;

#[test]
fn the_interface_language_holds_unless_another_one_is_chosen() {
    assert_eq!(resolve(None, Some("de")), Some(ReplyLanguage::De));
    assert_eq!(resolve(Some("ui"), Some("en")), Some(ReplyLanguage::En));
    assert_eq!(resolve(Some("fr"), Some("de")), Some(ReplyLanguage::Fr));
    assert_eq!(
        resolve(Some("klingonisch"), Some("de")),
        Some(ReplyLanguage::De)
    );
    assert_eq!(resolve(None, None), None);
}

#[test]
fn every_language_round_trips_through_its_code() {
    for language in [
        ReplyLanguage::De,
        ReplyLanguage::En,
        ReplyLanguage::Fr,
        ReplyLanguage::Es,
        ReplyLanguage::It,
        ReplyLanguage::Pt,
    ] {
        assert_eq!(ReplyLanguage::from_code(language.code()), Some(language));
    }
}

#[test]
fn a_typed_turn_takes_the_interface_language_and_a_spoken_one_its_own() {
    let (_dir, db) = open_test_vault();
    let device = uuid::Uuid::new_v4();
    db.write(|tx| {
        tx.execute(
            "INSERT INTO known_devices (installation_uuid, vault_device_uuid, alias, first_seen) \
             VALUES ('inst', ?1, NULL, 0)",
            haex_crdt::rusqlite::params![device.to_string()],
        )
    })
    .expect("device");
    let read = |db: &haex_crdt::Database| {
        query::read(db, |r| Ok((read_typed(r)?, read_spoken(r, device)?))).expect("read")
    };
    assert_eq!(read(&db), (None, None));
    db.write(|tx| preferences::insert_or_update(tx, PrefScope::Vault, PREF_UI_LANGUAGE, "de"))
        .expect("ui");
    assert_eq!(
        read(&db),
        (Some(ReplyLanguage::De), Some(ReplyLanguage::De))
    );
    db.write(|tx| {
        preferences::insert_or_update(
            tx,
            PrefScope::Device(device),
            PREF_CONVERSATION_LANGUAGE,
            "es",
        )
    })
    .expect("conversation");
    assert_eq!(
        read(&db),
        (Some(ReplyLanguage::De), Some(ReplyLanguage::Es))
    );
}
