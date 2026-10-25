//! The file browser on another own device (spec 056 US1, T037): browsing, reading, copying in,
//! renaming and deleting through `StorageFiles` over `PeerStore`, holzi's own data kept read-only,
//! the device switch, which leaves contacts in (US5), and a watched folder (scenarios 2, 4, 5, 7,
//! 8).

use std::sync::Arc;

use tokio::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

use crate::files::encrypted::test_support::transfer;
use crate::files::peer::store::{peer_access, PeerStore};
use crate::files::peer::test_support::{own_pair, store_card, Device, Fixed};
use crate::files::peer::wire::{read_head, Caller, FailCode, Mode, Op, Reply};
use crate::files::share::{device_access, FileShareItemKind, FileShareLevel};
use crate::files::storage_source::StorageFiles;
use crate::files::transfer::local::Outcome;
use crate::files::transfer::remote::Side;
use crate::files::transfer::{ConflictChoice, TransferOp};
use crate::files::{EntryKind, FilesErrorCode};
use crate::sync::test_support::Member;

/// `on` as a storage of `from`, asked by `caller`; a second handle on it for a transfer.
async fn device(
    from: &Device,
    on: &Device,
    caller: Caller,
) -> (StorageFiles, StorageFiles, PeerStore) {
    let link = Arc::new(from.link_to(on).await.expect("link"));
    let files = || {
        StorageFiles::new(
            Arc::new(PeerStore::new(Arc::new(Fixed(Arc::clone(&link))), caller)),
            peer_access(&format!("peer:{}", on.uuid), None),
        )
    };
    let (one, two) = (files(), files());
    (one, two, PeerStore::new(Arc::new(Fixed(link)), caller))
}

fn window(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

async fn delete(files: &StorageFiles, path: &str) -> Result<(), crate::files::FilesError> {
    files
        .delete(path, &CancellationToken::new(), &mut |_| {})
        .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_window_works_on_another_device_like_on_a_storage() {
    let (main, linked) = own_pair().await;
    let (files, target, _) = device(&linked, &main, Caller::User).await;
    let root = window(&main.root());

    files.create_folder(&root, "Briefe").await.expect("folder");
    assert!(main.root().join("Briefe").is_dir());

    let here = tempfile::tempdir().expect("here");
    let letter = here.path().join("brief.txt");
    std::fs::write(&letter, b"Hallo Welt").expect("local file");
    let outcome = transfer(
        TransferOp::Copy,
        Side::Device,
        &[window(&letter)],
        Side::Storage(Arc::new(target)),
        &format!("{root}/Briefe"),
        None,
        None,
    )
    .await
    .expect("copy");
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(
        std::fs::read(main.root().join("Briefe/brief.txt")).expect("arrived"),
        b"Hallo Welt"
    );

    let entries = files.list(&format!("{root}/Briefe")).await.expect("list");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, EntryKind::File);

    files
        .rename(&format!("{root}/Briefe/brief.txt"), "gruss.txt")
        .await
        .expect("rename");
    assert!(main.root().join("Briefe/gruss.txt").is_file());
    assert!(!main.root().join("Briefe/brief.txt").exists());

    delete(&files, &format!("{root}/Briefe"))
        .await
        .expect("delete");
    assert!(
        !main.root().join("Briefe").exists(),
        "gone on the serving device"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn copying_onto_a_taken_name_asks_and_keeps_both() {
    let (main, linked) = own_pair().await;
    main.write("Ablage/notiz.txt", b"alt");
    let (_, files, _) = device(&linked, &main, Caller::User).await;
    let here = tempfile::tempdir().expect("here");
    let note = here.path().join("notiz.txt");
    std::fs::write(&note, b"neu").expect("local file");

    let outcome = transfer(
        TransferOp::Copy,
        Side::Device,
        &[window(&note)],
        Side::Storage(Arc::new(files)),
        &window(&main.root().join("Ablage")),
        Some(ConflictChoice::KeepBoth),
        None,
    )
    .await
    .expect("copy");
    assert_eq!(outcome, Outcome::Done);
    let mut names: Vec<String> = std::fs::read_dir(main.root().join("Ablage"))
        .expect("folder")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    assert_eq!(names.len(), 2, "{names:?}");
    assert_eq!(
        std::fs::read(main.root().join("Ablage/notiz.txt")).expect("old"),
        b"alt",
        "the old file stays (FR-039)"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn holzis_own_data_is_read_but_never_changed_from_another_device() {
    let (main, linked) = own_pair().await;
    std::fs::write(main.own_place().join("vault.db"), b"x").expect("own file");
    let (files, _, _) = device(&linked, &main, Caller::User).await;
    let own = window(&main.own_place());

    assert_eq!(files.list(&own).await.expect("list").len(), 1);
    let refused = files.create_folder(&own, "neu").await.expect_err("create");
    assert_eq!(refused.code, FilesErrorCode::NoAccess);
    let refused = delete(&files, &format!("{own}/vault.db"))
        .await
        .expect_err("delete");
    assert_eq!(refused.code, FilesErrorCode::NoAccess);
    let refused = delete(&files, &own).await.expect_err("delete the place");
    assert_eq!(refused.code, FilesErrorCode::NoAccess);
    let refused = files
        .rename(&window(&main.root()), "anders")
        .await
        .expect_err("a folder holding an own place stays");
    assert!(
        matches!(
            refused.code,
            FilesErrorCode::NoAccess | FilesErrorCode::InvalidPath
        ),
        "{refused:?}"
    );
    assert!(main.own_place().join("vault.db").is_file());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn switching_off_refuses_changes_too() {
    let (main, linked) = own_pair().await;
    let (files, _, _) = device(&linked, &main, Caller::User).await;
    let root = window(&main.root());
    main.db
        .write_blocking(|tx| Ok(device_access::set_enabled(tx, false, "t")?))
        .expect("off");
    let refused = files.create_folder(&root, "neu").await.expect_err("create");
    assert_eq!(refused.code, FilesErrorCode::NoAccess);
    assert!(!main.root().join("neu").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_switch_turns_own_devices_away_and_leaves_contacts_in() {
    let (main, linked) = own_pair().await;
    let ben = Device::of(Member::genesis()).await;
    store_card(&ben, &main).await;
    main.write("Fotos/a.jpg", b"aaaa");
    let share = main.share(
        &main.root().join("Fotos"),
        FileShareItemKind::Folder,
        "Fotos",
    );
    main.grant(&share, ben.member.vault, FileShareLevel::Read);
    main.db
        .write_blocking(|tx| Ok(device_access::set_enabled(tx, false, "t")?))
        .expect("off");

    let link = linked.link_to(&main).await.expect("an answer");
    assert_eq!(link.welcome.mode, Mode::AccessOff, "US5 scenario 2");
    assert_eq!(
        link.ask(Caller::User, Op::Shares).await.expect("answer").0,
        Reply::Failed(FailCode::Refused)
    );

    let link = ben.link_to(&main).await.expect("contact link");
    assert_eq!(link.welcome.mode, Mode::Contact);
    let files = StorageFiles::new(
        Arc::new(PeerStore::new(
            Arc::new(Fixed(Arc::new(link))),
            Caller::User,
        )),
        peer_access("share:x", Some(&share)),
    );
    let names: Vec<String> = files
        .list("/")
        .await
        .expect("the share works on (scenario 4)")
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(names, ["a.jpg"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_new_file_in_a_watched_folder_arrives_within_two_seconds() {
    let (main, linked) = own_pair().await;
    std::fs::create_dir(main.root().join("Inbox")).expect("folder");
    let (_, _, store) = device(&linked, &main, Caller::User).await;
    let access = peer_access(&format!("peer:{}", main.uuid), None);
    let prefix = format!(
        "{}/",
        window(&main.root().join("Inbox")).trim_start_matches('/')
    );
    let mut changes = store
        .watch(&access, &prefix, Instant::now() + Duration::from_secs(5))
        .await
        .expect("watch");
    main.write("Inbox/neu.txt", b"hallo");
    let change = tokio::time::timeout(Duration::from_secs(2), read_head::<_, Reply>(&mut changes))
        .await
        .expect("within two seconds (SC-009)")
        .expect("a change");
    assert_eq!(change, Reply::Changed);
}
