//! Renaming and deleting in and of encrypted folders (T042): what each writes and deletes at the
//! provider, and what another device of the vault sees afterwards.

use tokio_util::sync::CancellationToken;

use super::*;
use crate::files::encrypted::format::sidecar::SidecarKind;
use crate::files::encrypted::test_support::{
    filled, since, tree, version_of, Scene, BUCKET, WRITES,
};
use crate::remote_storage::test_support::Op;
use crate::remote_storage::StorageError;

fn names(entries: &[Entry]) -> Vec<String> {
    let mut names: Vec<String> = entries.iter().map(|entry| entry.name.clone()).collect();
    names.sort();
    names
}

#[tokio::test(flavor = "multi_thread")]
async fn renaming_a_folder_of_a_thousand_files_writes_one_sidecar_and_no_content() {
    let files: Vec<(String, String)> = (0..1000)
        .map(|n| (format!("Rechnungen/r{n}.txt"), format!("Rechnung {n}")))
        .collect();
    let scene = filled(&files).await;
    let before = version_of(&scene.files(), "/Unterlagen/Rechnungen").await;
    let mark = scene.fake.calls().len();

    let entry = scene
        .files()
        .rename("/Unterlagen/Rechnungen", "Belege")
        .await
        .unwrap();
    assert_eq!(entry.path, "/Unterlagen/Belege");
    let writes = since(&scene, mark, &WRITES);
    assert_eq!(writes.len(), 1, "{writes:?}");
    assert!(writes[0].1.contains(".hxef/m/"), "{writes:?}");
    let deletes = since(&scene, mark, &[Op::Delete]);
    assert_eq!(deletes.len(), 1, "only the replaced sidecar: {deletes:?}");
    assert!(deletes[0].1.contains(".hxef/m/"));
    // SC-006: no content object written.
    assert!(!scene.fake.calls()[mark..]
        .iter()
        .any(|(op, key)| WRITES.contains(op) && key.contains(".hxef/c/")));

    // The new version replaces the old one (format: same entry, next revision, its base).
    let after = version_of(&scene.files(), "/Unterlagen/Belege").await;
    assert_eq!(after.sidecar.entry, before.sidecar.entry);
    assert_eq!(after.sidecar.revision, before.sidecar.revision + 1);
    assert_eq!(after.sidecar.base, Some(before.sid));

    let other = scene.fresh_files();
    assert_eq!(names(&other.list("/Unterlagen").await.unwrap()), ["Belege"]);
    assert_eq!(other.list("/Unterlagen/Belege").await.unwrap().len(), 1000);
    let text = other
        .read_start("/Unterlagen/Belege/r7.txt", 10, 100)
        .await
        .unwrap();
    assert_eq!(text, b"Rechnung 7");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_renamed_file_keeps_its_content_object() {
    let scene = filled(&tree(&[("notiz.txt", "Strom 2026")])).await;
    let contents = |scene: &Scene| -> Vec<String> {
        scene
            .keys()
            .into_iter()
            .filter(|key| key.contains(".hxef/c/"))
            .collect()
    };
    let before = contents(&scene);
    scene
        .files()
        .rename("/Unterlagen/notiz.txt", "strom.txt")
        .await
        .unwrap();
    assert_eq!(contents(&scene), before);
    let other = scene.fresh_files();
    assert_eq!(
        names(&other.list("/Unterlagen").await.unwrap()),
        ["strom.txt"]
    );
    let text = other
        .read_start("/Unterlagen/strom.txt", 10, 100)
        .await
        .unwrap();
    assert_eq!(text, b"Strom 2026");
}

#[tokio::test(flavor = "multi_thread")]
async fn renaming_the_encrypted_folder_writes_only_its_head() {
    let scene = filled(&tree(&[("Belege/a.txt", "a"), ("b.txt", "b")])).await;
    let keys = scene.keys();
    let mark = scene.fake.calls().len();
    let entry = scene.files().rename("/Unterlagen", "Akten").await.unwrap();
    assert_eq!(entry.name, "Akten");
    assert_eq!(entry.path, "/Akten");
    let writes = since(&scene, mark, &WRITES);
    assert_eq!(writes.len(), 1, "{writes:?}");
    assert!(writes[0].1.ends_with(".hxef/h"));
    assert!(since(&scene, mark, &[Op::Delete]).is_empty());
    assert_eq!(
        scene.keys(),
        keys,
        "the same objects, the head changed in place"
    );

    let other = scene.fresh_files();
    assert_eq!(names(&other.list("/").await.unwrap()), ["Akten"]);
    assert_eq!(
        names(&other.list("/Akten").await.unwrap()),
        ["Belege", "b.txt"]
    );
    // The window that renamed it finds it by its new name too.
    assert_eq!(scene.files().list("/Akten/Belege").await.unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rename_refuses_a_taken_name_and_a_damaged_entry() {
    let scene = filled(&tree(&[("a.txt", "a"), ("b.txt", "b")])).await;
    scene.fake.insert(BUCKET, "offen.txt", b"offen");
    let files = scene.files();
    let error = files
        .rename("/Unterlagen/a.txt", "b.txt")
        .await
        .unwrap_err();
    assert_eq!(error.code, FilesErrorCode::Exists);
    let error = files.rename("/Unterlagen", "offen.txt").await.unwrap_err();
    assert_eq!(error.code, FilesErrorCode::Exists);
    let error = files.rename("/Unterlagen/a.txt", "x/y").await.unwrap_err();
    assert_eq!(error.code, FilesErrorCode::InvalidName);

    // a.txt loses its content object: it is damaged and can only be deleted.
    let version = version_of(&files, "/Unterlagen/a.txt").await;
    let SidecarKind::File(facts) = &version.sidecar.kind else {
        panic!("a file");
    };
    let content = scene
        .keys()
        .into_iter()
        .find(|key| key.ends_with(&encode_id(&facts.content)))
        .unwrap();
    scene.fake.insert(BUCKET, &content, b"short");
    let other = scene.fresh_files();
    let error = other
        .rename("/Unterlagen/a.txt", "c.txt")
        .await
        .unwrap_err();
    assert_eq!(error.code, FilesErrorCode::Damaged);
    let cancel = CancellationToken::new();
    other
        .delete("/Unterlagen/a.txt", &cancel, &mut |_| {})
        .await
        .unwrap();
    assert_eq!(names(&other.list("/Unterlagen").await.unwrap()), ["b.txt"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_an_entry_removes_everything_below_it_its_own_sidecar_last() {
    let scene = filled(&tree(&[
        ("Belege/2026/strom.pdf", "pdf"),
        ("Belege/gas.txt", "gas"),
        ("notiz.txt", "bleibt"),
    ]))
    .await;
    let own = version_of(&scene.files(), "/Unterlagen/Belege").await;
    let mark = scene.fake.calls().len();
    let mut progress = Vec::new();
    let cancel = CancellationToken::new();
    scene
        .files()
        .delete("/Unterlagen/Belege", &cancel, &mut |n| progress.push(n))
        .await
        .unwrap();
    let deletes = since(&scene, mark, &[Op::Delete]);
    // strom.pdf and gas.txt (content and sidecar each), 2026 and Belege (sidecars).
    assert_eq!(deletes.len(), 6, "{deletes:?}");
    assert_eq!(progress.last(), Some(&6));
    let own_key = format!("m/{}", encode_id(&own.sid));
    assert!(deletes.last().unwrap().1.ends_with(&own_key), "{deletes:?}");
    let left: Vec<String> = scene
        .keys()
        .into_iter()
        .filter(|key| key.contains("/m/") || key.contains("/c/"))
        .collect();
    assert_eq!(left.len(), 2, "notiz.txt stays: {left:?}");
    let other = scene.fresh_files();
    assert_eq!(
        names(&other.list("/Unterlagen").await.unwrap()),
        ["notiz.txt"]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_replaced_version_left_behind_does_not_show_again_after_a_delete() {
    let scene = filled(&tree(&[("notiz.txt", "eins")])).await;
    let first = version_of(&scene.files(), "/Unterlagen/notiz.txt").await;
    // The old sidecar stays behind after the rename (its delete fails).
    scene.fake.fail_once(Op::Delete, StorageError::Network);
    scene
        .files()
        .rename("/Unterlagen/notiz.txt", "zwei.txt")
        .await
        .unwrap();
    let first_key = format!("m/{}", encode_id(&first.sid));
    assert!(scene.keys().iter().any(|key| key.ends_with(&first_key)));
    let other = scene.fresh_files();
    assert_eq!(
        names(&other.list("/Unterlagen").await.unwrap()),
        ["zwei.txt"]
    );

    let cancel = CancellationToken::new();
    other
        .delete("/Unterlagen/zwei.txt", &cancel, &mut |_| {})
        .await
        .unwrap();
    assert!(scene
        .fresh_files()
        .list("/Unterlagen")
        .await
        .unwrap()
        .is_empty());
    assert!(!scene
        .keys()
        .iter()
        .any(|key| key.contains("/m/") || key.contains("/c/")));
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_the_folder_removes_everything_under_its_prefix_the_head_last() {
    let scene = filled(&tree(&[("Belege/a.txt", "a"), ("b.txt", "b")])).await;
    scene.fake.insert(BUCKET, "offen.txt", b"offen");
    let mark = scene.fake.calls().len();
    let cancel = CancellationToken::new();
    scene
        .files()
        .delete("/Unterlagen", &cancel, &mut |_| {})
        .await
        .unwrap();
    let deletes = since(&scene, mark, &[Op::Delete]);
    assert_eq!(deletes.len(), 6, "h, 3 sidecars, 2 contents: {deletes:?}");
    assert!(deletes.last().unwrap().1.ends_with(".hxef/h"));
    assert_eq!(scene.keys(), ["offen.txt"]);
    assert_eq!(
        names(&scene.files().list("/").await.unwrap()),
        ["offen.txt"]
    );
    let cached = scene
        .db
        .read(|q| {
            use crate::storage::query::Query;
            q.query_map(
                "SELECT COUNT(*) FROM encrypted_folder_entries_no_sync",
                &[],
                |r| r.get::<_, i64>(0),
            )
        })
        .await
        .unwrap();
    assert_eq!(cached, [0], "the cache forgets the folder");
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_the_folder_takes_its_foreign_files_with_it() {
    let scene = filled(&tree(&[("b.txt", "b")])).await;
    let head = scene
        .keys()
        .into_iter()
        .find(|key| key.ends_with(".hxef/h"))
        .unwrap();
    let foreign = format!("{}notiz (conflicted copy).txt", &head[..head.len() - 1]);
    scene.fake.insert(BUCKET, &foreign, b"fremd");
    let cancel = CancellationToken::new();
    scene
        .files()
        .delete("/Unterlagen", &cancel, &mut |_| {})
        .await
        .unwrap();
    assert!(scene.keys().is_empty(), "{:?}", scene.keys());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unreadable_encrypted_folder_can_still_be_deleted() {
    let scene = filled(&tree(&[("a.txt", "a")])).await;
    let head = scene
        .keys()
        .into_iter()
        .find(|key| key.ends_with(".hxef/h"))
        .unwrap();
    scene.fake.insert(BUCKET, &head, b"HXEF\x01 kaputt");
    let other = scene.fresh_files();
    let listed = other.list("/").await.unwrap();
    assert_eq!(listed.len(), 1);
    let cancel = CancellationToken::new();
    other
        .delete(&listed[0].path, &cancel, &mut |_| {})
        .await
        .unwrap();
    assert!(scene.keys().is_empty());
}
