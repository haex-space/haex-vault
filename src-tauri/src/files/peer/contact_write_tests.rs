//! Contacts write into a share (spec 056 US3, T057): two contacts with "Lesen und Schreiben" create,
//! rename, move and delete below the root; the root itself stays; a share of one file takes only new
//! content; a change at the owner during an upload ends it in a conflict that keeps the owner's file.

use tokio_util::sync::CancellationToken;

use super::contact_tests::{contact_of, files_of, links_of, soon};
use crate::files::peer::contacts;
use crate::files::peer::test_support::{store_card, Device};
use crate::files::share::{FileShareItemKind, FileShareLevel};
use crate::remote_storage::{ObjectStore, StorageError};
use crate::sync::test_support::Member;

/// The storage id of the one share `anna` gave `contact`, as `contact` finds it.
async fn found_by(contact: &Device, anna: &Device) -> String {
    let links = links_of(contact, anna).await;
    let id = contact_of(anna, contact).await;
    let found = contacts::query(&contact.db, &links, id)
        .await
        .expect("query");
    assert_eq!(found.shares.len(), 1);
    found.shares[0].storage_id.clone()
}

/// Anna and `count` contacts who know each other's cards.
async fn anna_and(count: usize) -> (Device, Vec<Device>) {
    let anna = Device::of(Member::genesis()).await;
    let mut contacts = Vec::new();
    for _ in 0..count {
        let contact = Device::of(Member::genesis()).await;
        store_card(&anna, &contact).await;
        store_card(&contact, &anna).await;
        contacts.push(contact);
    }
    (anna, contacts)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_contacts_write_into_one_share() {
    let (anna, contacts) = anna_and(2).await;
    let (ben, cleo) = (&contacts[0], &contacts[1]);
    anna.write("Projekt/plan.txt", b"v1");
    let share = anna.share(
        &anna.root().join("Projekt"),
        FileShareItemKind::Folder,
        "Projekt",
    );
    for contact in [ben, cleo] {
        anna.grant(&share, contact.member.vault, FileShareLevel::ReadWrite);
    }
    let id = found_by(ben, &anna).await;
    let (bens, _) = files_of(ben, links_of(ben, &anna).await, &id).await;
    let id = found_by(cleo, &anna).await;
    let (cleos, cleo_store) = files_of(cleo, links_of(cleo, &anna).await, &id).await;

    bens.create_folder("/", "Entwürfe").await.expect("folder");
    cleos
        .rename("/plan.txt", "plan-alt.txt")
        .await
        .expect("rename");
    cleo_store
        .rename_tree(
            cleos.access(),
            "plan-alt.txt",
            "Entwürfe/plan-alt.txt",
            soon(),
        )
        .await
        .expect("move");
    let none = &mut |_| {};
    bens.delete("/Entwürfe/plan-alt.txt", &CancellationToken::new(), none)
        .await
        .expect("delete");

    let projekt = anna.root().join("Projekt");
    assert!(projekt.join("Entwürfe").is_dir());
    assert!(!projekt.join("plan.txt").exists());
    assert!(!projekt.join("Entwürfe/plan-alt.txt").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_root_of_a_share_is_neither_moved_nor_deleted() {
    let (anna, contacts) = anna_and(1).await;
    let ben = &contacts[0];
    anna.write("Projekt/plan.txt", b"v1");
    let share = anna.share(
        &anna.root().join("Projekt"),
        FileShareItemKind::Folder,
        "Projekt",
    );
    anna.grant(&share, ben.member.vault, FileShareLevel::ReadWrite);
    let id = found_by(ben, &anna).await;
    let (files, store) = files_of(ben, links_of(ben, &anna).await, &id).await;
    let access = files.access().clone();

    assert_eq!(
        store.remove_tree(&access, "", soon()).await,
        Err(StorageError::MissingRight)
    );
    assert_eq!(
        store.rename_tree(&access, "", "Neu/", soon()).await,
        Err(StorageError::MissingRight)
    );
    assert!(anna.root().join("Projekt/plan.txt").exists(), "FR-037");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_shared_file_takes_new_content_and_nothing_else() {
    let (anna, contacts) = anna_and(1).await;
    let ben = &contacts[0];
    let bericht = anna.write("Bericht.txt", b"v1");
    let share = anna.share(&bericht, FileShareItemKind::File, "Bericht");
    anna.grant(&share, ben.member.vault, FileShareLevel::ReadWrite);
    let id = found_by(ben, &anna).await;
    let (files, store) = files_of(ben, links_of(ben, &anna).await, &id).await;
    let access = files.access().clone();

    store
        .put(&access, "Bericht.txt", b"v2".to_vec(), soon())
        .await
        .expect("new content");
    assert_eq!(std::fs::read(&bericht).expect("read"), b"v2");

    for refused in [
        store
            .rename_tree(&access, "Bericht.txt", "Anders.txt", soon())
            .await,
        store.delete(&access, "Bericht.txt", soon()).await,
        store.put(&access, "Neu.txt", b"x".to_vec(), soon()).await,
    ] {
        assert!(refused.is_err(), "{refused:?}");
    }
    assert_eq!(std::fs::read(&bericht).expect("still there"), b"v2");
    assert!(!anna.root().join("Anders.txt").exists());
    assert!(!anna.root().join("Neu.txt").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_change_at_the_owner_during_an_upload_ends_in_a_conflict() {
    let (anna, contacts) = anna_and(1).await;
    let ben = &contacts[0];
    anna.write("Projekt/plan.txt", b"v1");
    let share = anna.share(
        &anna.root().join("Projekt"),
        FileShareItemKind::Folder,
        "Projekt",
    );
    anna.grant(&share, ben.member.vault, FileShareLevel::ReadWrite);
    let id = found_by(ben, &anna).await;
    let (files, store) = files_of(ben, links_of(ben, &anna).await, &id).await;
    let access = files.access().clone();

    let upload = store
        .begin_upload(&access, "plan.txt", soon())
        .await
        .expect("begin over v1");
    let mark = store
        .upload_part(&access, "plan.txt", &upload, 1, b"von Ben".to_vec(), soon())
        .await
        .expect("part");
    anna.write("Projekt/plan.txt", b"von Anna, inzwischen");
    assert_eq!(
        store
            .finish_upload(&access, "plan.txt", &upload, &[mark], soon())
            .await,
        Err(StorageError::Conflict),
        "FR-036"
    );
    assert_eq!(
        std::fs::read(anna.root().join("Projekt/plan.txt")).expect("read"),
        b"von Anna, inzwischen",
        "the owner's change stays"
    );
    let names: Vec<String> = files
        .list("/")
        .await
        .expect("list")
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(names, ["plan.txt"], "no half file left");
}
