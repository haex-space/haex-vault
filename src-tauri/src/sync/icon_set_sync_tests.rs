//! The icon sets over the sync (spec 053 US4, T049): a custom set arrives whole or not at all, a
//! delete reaches the other device, two devices that choose at the same time end with one choice,
//! and a pack changed on the way is damaged on the receiving device and shows the fallback.

// These tests read `haex_file_icon_sets` with the CRDT write path where they insert rows.
#![allow(clippy::disallowed_methods)]

use std::io::{Cursor, Write};

use crate::files::icons::store::{self, NewSet};
use crate::files::icons::{pack_hash, IconSetRef, IconSetState, SELECTION_KEY};
use crate::storage::preferences::{self, PrefScope};
use crate::storage::query;
use crate::sync::inbound::Inbox;
use crate::sync::outbound::serve_pull_with_budget;
use crate::sync::test_support::Device;

const ID: &str = "0b9a2c52-6a53-4f4e-9a1b-3c1a1d0a77aa";

/// A valid pack of one image, so a damaged result is the hash's doing.
fn small_pack() -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    writer
        .start_file("icon-theme.json", options)
        .expect("start");
    writer
        .write_all(br#"{"file": "a", "iconDefinitions": {"a": {"iconPath": "a.svg"}}}"#)
        .expect("write");
    writer.start_file("a.svg", options).expect("start");
    writer
        .write_all(br#"<svg xmlns="http://www.w3.org/2000/svg"/>"#)
        .expect("write");
    writer.finish().expect("finish").into_inner()
}

/// About a mebibyte that no compression shrinks and that no pack check cares about: the sync moves
/// the bytes, it does not look inside.
fn big_blob() -> Vec<u8> {
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    (0..1024 * 1024)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 33) as u8
        })
        .collect()
}

fn insert(device: &Device, id: &str, pack: &[u8], hash: &str) {
    device
        .db()
        .write(|tx| {
            store::insert(
                tx,
                id,
                &NewSet {
                    name: "Demo",
                    author: "",
                    license: "",
                    source: "",
                    image_count: 1,
                    sha256: hash,
                    pack,
                    created_at: "2026-10-15T00:00:00Z",
                },
            )
        })
        .expect("insert the set");
}

fn rows(device: &Device) -> usize {
    query::read(device.db(), |q| store::count(q)).expect("count")
}

fn pack_of(device: &Device, id: &str) -> Option<Vec<u8>> {
    query::read(device.db(), |q| store::pack_of(q, id))
        .expect("read the pack")
        .map(|(_, bytes)| bytes)
}

fn state(device: &Device, id: &str) -> IconSetState {
    let set = IconSetRef::Custom { id: id.to_owned() };
    query::read(device.db(), |q| store::in_use(q, &set))
        .expect("in use")
        .state
}

fn set_choice(device: &Device, value: &str) {
    device
        .db()
        .write(|tx| {
            preferences::insert_or_update(tx, PrefScope::Vault, SELECTION_KEY, value).map(|_| ())
        })
        .expect("choose");
}

fn choice(device: &Device) -> Option<String> {
    query::read(device.db(), |q| store::selection_value(q)).expect("choice")
}

#[test]
fn a_set_of_a_mebibyte_arrives_whole_or_not_at_all() {
    let (a, b) = (Device::new(), Device::new());
    let blob = big_blob();
    insert(&a, ID, &blob, &pack_hash(&blob));

    // Pages of 64 KiB: the row comes in parts, and the other device shows it only after the last.
    let (theirs, floors) = b.replica.pull_vector().expect("vector");
    let mut outbox = serve_pull_with_budget(&a.replica, &theirs, 64 * 1024).expect("serve");
    let mut inbox = Inbox::new().refetching(floors);
    let mut pages = 0;
    while let Some(page) = outbox.next_page() {
        let last = !page.more;
        inbox.receive(&b.replica, page).expect("receive");
        pages += 1;
        if !last {
            assert_eq!(rows(&b), 0, "page {pages}: a part of a set is not a set");
        }
    }
    assert!(pages > 1, "the set needed several pages, not {pages}");
    assert_eq!(rows(&b), 1);
    assert_eq!(pack_of(&b, ID).as_deref(), Some(blob.as_slice()));
}

#[test]
fn a_delete_leaves_a_marker_and_removes_the_row_on_the_other_device() {
    let (a, b) = (Device::new(), Device::new());
    let pack = small_pack();
    insert(&a, ID, &pack, &pack_hash(&pack));
    b.pull_from(&a);
    assert_eq!(rows(&b), 1);
    assert_eq!(state(&b, ID), IconSetState::Ready);

    a.db()
        .write(|tx| store::delete(tx, ID))
        .expect("delete the set");
    b.pull_from(&a);

    assert_eq!(rows(&b), 0, "the delete reached the other device");
    assert_eq!(
        state(&b, ID),
        IconSetState::Missing,
        "the marker tells a deleted set from one that has not arrived"
    );
}

#[test]
fn two_devices_that_choose_at_the_same_time_end_with_the_same_choice() {
    let (a, b) = (Device::new(), Device::new());
    set_choice(&a, "builtin:phosphor");
    set_choice(&b, "builtin:material");

    b.pull_from(&a);
    a.pull_from(&b);
    b.pull_from(&a);

    assert_eq!(choice(&a), choice(&b));
    assert!(choice(&a).is_some());
}

#[test]
fn a_pack_changed_on_the_way_is_damaged_and_the_choice_shows_the_fallback() {
    let (a, b) = (Device::new(), Device::new());
    let pack = small_pack();
    let hash = pack_hash(&pack);
    let mut flipped = pack.clone();
    let middle = flipped.len() / 2;
    flipped[middle] ^= 0x01;
    // The row on the sending side carries the hash of the original and the bytes of the changed one.
    insert(&a, ID, &flipped, &hash);
    set_choice(&a, &format!("custom:{ID}"));

    b.pull_from(&a);

    assert_eq!(rows(&b), 1, "the row itself arrived");
    assert_eq!(state(&b, ID), IconSetState::Damaged);
    let (set, in_use) =
        query::read(b.db(), |q| store::chosen(q)).expect("the choice of the other device");
    assert_eq!(set, IconSetRef::Custom { id: ID.to_owned() });
    assert_eq!(in_use.state, IconSetState::Damaged);
    assert!(matches!(in_use.source, store::Source::Lucide));
    // An untouched pack of the same device is ready.
    let other = "5d2f6c4e-1f3a-4a45-8d3e-6f1d2b9c8e10";
    insert(&a, other, &pack, &hash);
    b.pull_from(&a);
    assert_eq!(state(&b, other), IconSetState::Ready);
}

#[test]
fn a_set_that_has_not_arrived_yet_is_syncing_not_missing() {
    let (a, b) = (Device::new(), Device::new());
    set_choice(&a, &format!("custom:{ID}"));
    b.pull_from(&a);
    assert_eq!(state(&b, ID), IconSetState::Syncing);
}
