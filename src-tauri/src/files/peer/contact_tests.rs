//! A contact opens a share (spec 056 US2, T051): it finds the share by asking the devices on the
//! owner's card, reads through `PeerStore` below the root only, cannot write at level read, a
//! second device of its vault gets in without anything done at the owner, and a watched shared
//! folder reports changes (scenarios 2–4, 6).

use std::sync::Arc;

use tokio::time::{Duration, Instant};

use crate::contacts::card::received as cards;
use crate::files::encrypted::test_support::transfer;
use crate::files::peer::connect::Connections;
use crate::files::peer::contacts;
use crate::files::peer::store::{peer_access, PeerStore};
use crate::files::peer::test_support::{store_card, Device};
use crate::files::peer::wire::{read_head, Caller, Reply};
use crate::files::share::{FileShareItemKind, FileShareLevel};
use crate::files::storage_source::StorageFiles;
use crate::files::transfer::local::Outcome;
use crate::files::transfer::remote::Side;
use crate::files::transfer::{ConflictChoice, TransferOp};
use crate::files::FilesErrorCode;
use crate::remote_storage::{ObjectStore, StorageError};
use crate::sync::test_support::Member;

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(20)
}

/// The contact that carries `of`'s card on `on`.
async fn contact_of(of: &Device, on: &Device) -> String {
    let vault = of.member.vault;
    on.db
        .read(move |q| Ok(cards::stored(q, &vault)?))
        .await
        .expect("card")
        .expect("stored")
        .0
}

/// Ben's links, with Anna's device findable at its loopback address.
async fn links_of(ben: &Device, anna: &Device) -> Connections {
    ben.node.note_presence(anna.node.addr()).await;
    Connections::new(Arc::clone(&ben.node), Arc::clone(&ben.ctx))
}

/// Anna shares `Fotos` with Ben at `level`; Ben asks Anna's devices and gets its storage id.
async fn shared(level: FileShareLevel) -> (Device, Device, Connections, String) {
    let anna = Device::of(Member::genesis()).await;
    let ben = Device::of(Member::genesis()).await;
    store_card(&anna, &ben).await;
    store_card(&ben, &anna).await;
    anna.write("Fotos/a.jpg", b"aaaa");
    anna.write("Geheim/c.txt", b"secret");
    let share = anna.share(
        &anna.root().join("Fotos"),
        FileShareItemKind::Folder,
        "Fotos",
    );
    anna.grant(&share, ben.member.vault, level);
    let links = links_of(&ben, &anna).await;
    let contact = contact_of(&anna, &ben).await;
    let found = contacts::query(&ben.db, &links, contact)
        .await
        .expect("query");
    assert_eq!(found.shares.len(), 1);
    assert!(found.shares[0].reachable);
    assert_eq!(found.shares[0].name, "Fotos");
    let storage_id = found.shares[0].storage_id.clone();
    (anna, ben, links, storage_id)
}

async fn files_of(ben: &Device, links: Connections, storage_id: &str) -> (StorageFiles, PeerStore) {
    let share = contacts::share_of(&ben.db, storage_id)
        .await
        .expect("share");
    let links = Arc::new(links);
    let access = peer_access(storage_id, Some(&share));
    let files = StorageFiles::new(
        Arc::new(PeerStore::new(Arc::clone(&links) as _, Caller::User)),
        access,
    );
    (files, PeerStore::new(links, Caller::User))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_contact_reads_its_share_and_nothing_beside_it() {
    let (anna, ben, links, storage_id) = shared(FileShareLevel::Read).await;
    let (files, store) = files_of(&ben, links, &storage_id).await;

    let entries = files.list("/").await.expect("the root");
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["a.jpg"]);
    for outside in ["/..", "/../Geheim"] {
        assert!(files.list(outside).await.is_err(), "{outside}");
    }
    let access = files.access().clone();
    assert_eq!(
        store
            .get(&access, "a.jpg", 100, soon())
            .await
            .expect("read"),
        b"aaaa"
    );

    let refused = files
        .create_folder("/", "neu")
        .await
        .expect_err("read only");
    assert_eq!(refused.code, FilesErrorCode::NoAccess);
    assert_eq!(
        store.put(&access, "b.jpg", b"x".to_vec(), soon()).await,
        Err(StorageError::MissingRight),
        "refused when sent directly too"
    );
    assert!(!anna.root().join("Fotos/b.jpg").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn another_device_of_the_contact_gets_in_without_anything_done_at_the_owner() {
    let anna = Device::of(Member::genesis()).await;
    let ben = Member::genesis();
    let tablet = Member::join(&ben);
    ben.add(&tablet);
    // Sync brings the list that names the tablet to it.
    tablet.device.pull_from(&ben.device);
    let ben = Device::of(ben).await;
    let tablet = Device::of(tablet).await;
    store_card(&ben, &anna).await;
    store_card(&anna, &tablet).await;
    anna.write("Fotos/a.jpg", b"aaaa");
    let share = anna.share(
        &anna.root().join("Fotos"),
        FileShareItemKind::Folder,
        "Fotos",
    );
    anna.grant(&share, ben.member.vault, FileShareLevel::Read);

    let links = links_of(&tablet, &anna).await;
    let contact = contact_of(&anna, &tablet).await;
    let found = contacts::query(&tablet.db, &links, contact)
        .await
        .expect("query");
    assert_eq!(found.shares.len(), 1, "the grant is the vault's (FR-004)");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_contact_writes_into_a_shared_folder_and_sees_its_changes() {
    let (anna, ben, links, storage_id) = shared(FileShareLevel::ReadWrite).await;
    let (files, store) = files_of(&ben, links, &storage_id).await;
    let (target, _) = files_of(&ben, links_of(&ben, &anna).await, &storage_id).await;

    let here = tempfile::tempdir().expect("here");
    let photo = here.path().join("a.jpg");
    std::fs::write(&photo, b"neu").expect("local");
    let outcome = transfer(
        TransferOp::Copy,
        Side::Device,
        &[photo.to_string_lossy().into_owned()],
        Side::Storage(Arc::new(target)),
        "/",
        Some(ConflictChoice::KeepBoth),
        None,
    )
    .await
    .expect("copy");
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(
        std::fs::read(anna.root().join("Fotos/a.jpg")).expect("old"),
        b"aaaa",
        "both kept (FR-039)"
    );
    assert_eq!(
        std::fs::read_dir(anna.root().join("Fotos"))
            .expect("dir")
            .count(),
        2
    );

    let mut changes = store
        .watch(files.access(), "", soon())
        .await
        .expect("watch");
    anna.write("Fotos/neu.jpg", b"n");
    let change = tokio::time::timeout(Duration::from_secs(2), read_head::<_, Reply>(&mut changes))
        .await
        .expect("within two seconds")
        .expect("a change");
    assert_eq!(change, Reply::Changed);
}
