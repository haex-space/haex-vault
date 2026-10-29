//! Transfers in, out of and between encrypted folders that US5 brings (T044, T046): moves inside one
//! folder write one sidecar per moved entry, copies between encrypted folders of one bucket copy the
//! content object at the provider, the folder itself moves object by object into an ordinary
//! folder, moves out remove what they took, files leaving encryption are counted for the question,
//! and an encrypted folder inside an ordinary one goes to the device decrypted.

use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::files::encrypted::format::sidecar::SidecarKind;
use crate::files::encrypted::ops::create_encrypted_folder;
use crate::files::encrypted::test_support::{
    filled, since, transfer, tree, version_of, Scene, BUCKET, WRITES,
};
use crate::files::storage_source::StorageFiles;
use crate::files::transfer::local::Outcome;
use crate::files::transfer::remote::{prepare_remote, Plan, RemoteJob, Side};
use crate::files::transfer::{
    confirm_leaving, ConflictChoice, LeaveChoice, Leaving, TransferAnswer, TransferEvent,
    TransferOp,
};
use crate::files::Entry;
use crate::remote_storage::test_support::Op;

fn side(files: StorageFiles) -> Side {
    Side::Storage(Arc::new(files))
}

fn device() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let real = std::fs::canonicalize(dir.path()).unwrap();
    (dir, real)
}

fn names(entries: &[Entry]) -> Vec<String> {
    let mut names: Vec<String> = entries.iter().map(|entry| entry.name.clone()).collect();
    names.sort();
    names
}

async fn within(
    scene: &Scene,
    op: TransferOp,
    sources: &[&str],
    target: &str,
    answer: Option<ConflictChoice>,
) -> Outcome {
    let sources: Vec<String> = sources.iter().map(|s| (*s).to_owned()).collect();
    transfer(
        op,
        side(scene.files()),
        &sources,
        side(scene.files()),
        target,
        answer,
        None,
    )
    .await
    .unwrap()
}

async fn plan(scene: &Scene, sources: &[&str], to: Side, target: &str) -> Plan {
    let job = RemoteJob {
        op: TransferOp::Copy,
        from: side(scene.files()),
        sources: sources.iter().map(|s| (*s).to_owned()).collect(),
        to,
        target: target.to_owned(),
        known_free: false,
    };
    tokio::task::spawn_blocking(move || {
        prepare_remote(&job, &tokio::runtime::Handle::current(), |_| None).unwrap()
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_move_inside_the_folder_writes_one_sidecar_per_moved_entry() {
    let files: Vec<(String, String)> = (0..50)
        .map(|n| (format!("Rechnungen/r{n}.txt"), format!("Rechnung {n}")))
        .collect();
    let scene = filled(&files).await;
    scene
        .files()
        .create_folder("/Unterlagen", "Archiv")
        .await
        .unwrap();
    let mark = scene.fake.calls().len();
    let moved = within(
        &scene,
        TransferOp::Move,
        &["/Unterlagen/Rechnungen"],
        "/Unterlagen/Archiv",
        None,
    )
    .await;
    assert_eq!(moved, Outcome::Done);
    let writes = since(&scene, mark, &WRITES);
    assert_eq!(writes.len(), 1, "{writes:?}");
    assert!(writes[0].1.contains(".hxef/m/"));
    assert_eq!(since(&scene, mark, &[Op::Delete]).len(), 1);

    let other = scene.fresh_files();
    assert_eq!(names(&other.list("/Unterlagen").await.unwrap()), ["Archiv"]);
    let inside = other.list("/Unterlagen/Archiv/Rechnungen").await.unwrap();
    assert_eq!(inside.len(), 50);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_moved_file_onto_a_taken_name_asks_and_a_folder_merges() {
    let scene = filled(&tree(&[
        ("a.txt", "neu"),
        ("Belege/b.txt", "b"),
        ("Archiv/a.txt", "alt"),
        ("Archiv/Belege/c.txt", "c"),
    ]))
    .await;
    let moved = within(
        &scene,
        TransferOp::Move,
        &["/Unterlagen/a.txt", "/Unterlagen/Belege"],
        "/Unterlagen/Archiv",
        Some(ConflictChoice::Replace),
    )
    .await;
    assert_eq!(moved, Outcome::Done);
    let other = scene.fresh_files();
    assert_eq!(names(&other.list("/Unterlagen").await.unwrap()), ["Archiv"]);
    assert_eq!(
        names(&other.list("/Unterlagen/Archiv").await.unwrap()),
        ["Belege", "a.txt"]
    );
    assert_eq!(
        names(&other.list("/Unterlagen/Archiv/Belege").await.unwrap()),
        ["b.txt", "c.txt"]
    );
    let text = other
        .read_start("/Unterlagen/Archiv/a.txt", 3, 10)
        .await
        .unwrap();
    assert_eq!(text, b"neu");
    let contents = scene
        .keys()
        .into_iter()
        .filter(|key| key.contains(".hxef/c/"))
        .count();
    assert_eq!(contents, 3, "the replaced file's content is gone");
}

#[tokio::test(flavor = "multi_thread")]
async fn copies_inside_and_between_encrypted_folders_copy_the_content_at_the_provider() {
    let scene = filled(&tree(&[("notiz.txt", "Strom 2026"), ("Kopien/x.txt", "x")])).await;
    create_encrypted_folder(&scene.files(), "/", "Andere")
        .await
        .unwrap();
    let mark = scene.fake.calls().len();
    for target in ["/Unterlagen/Kopien", "/Andere"] {
        let copied = within(
            &scene,
            TransferOp::Copy,
            &["/Unterlagen/notiz.txt"],
            target,
            None,
        )
        .await;
        assert_eq!(copied, Outcome::Done, "{target}");
    }
    let copies = since(&scene, mark, &[Op::Copy]);
    assert_eq!(copies.len(), 2, "{copies:?}");
    assert!(copies.iter().all(|(_, key)| key.contains(".hxef/c/")));
    assert!(!since(&scene, mark, &WRITES)
        .iter()
        .any(|(op, key)| *op != Op::Copy && key.contains(".hxef/c/")));

    let files = scene.fresh_files();
    let original = version_of(&files, "/Unterlagen/notiz.txt").await;
    for path in ["/Unterlagen/Kopien/notiz.txt", "/Andere/notiz.txt"] {
        let copy = version_of(&files, path).await;
        let (SidecarKind::File(a), SidecarKind::File(b)) =
            (&original.sidecar.kind, &copy.sidecar.kind)
        else {
            panic!("files");
        };
        assert_ne!(copy.sidecar.entry, original.sidecar.entry, "{path}");
        assert_ne!(b.content, a.content, "a new oid: {path}");
        assert_eq!(b.cid, a.cid, "the same cid: {path}");
        assert_ne!(b.dek, a.dek, "the DEK wrapped again: {path}");
        assert_eq!(
            files.read_start(path, 10, 100).await.unwrap(),
            b"Strom 2026"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_encrypted_folder_moves_into_an_ordinary_folder_object_by_object() {
    let scene = filled(&tree(&[("Belege/a.txt", "a"), ("b.txt", "b")])).await;
    scene.fake.insert(BUCKET, "Archiv/", b"");
    let before: Vec<String> = scene
        .keys()
        .into_iter()
        .filter(|k| k != "Archiv/")
        .collect();
    let mark = scene.fake.calls().len();
    let moved = transfer(
        TransferOp::Move,
        side(scene.files()),
        &["/Unterlagen".to_owned()],
        side(scene.files()),
        "/Archiv",
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(moved, Outcome::Done);
    let copies = since(&scene, mark, &[Op::Copy]);
    assert_eq!(copies.len(), before.len());
    assert!(copies.last().unwrap().1.ends_with(".hxef/h"), "{copies:?}");
    let deletes = since(&scene, mark, &[Op::Delete]);
    assert!(
        deletes.last().unwrap().1.ends_with(".hxef/h"),
        "{deletes:?}"
    );
    let mut expected: Vec<String> = before.iter().map(|key| format!("Archiv/{key}")).collect();
    expected.push("Archiv/".to_owned());
    expected.sort();
    assert_eq!(scene.keys(), expected);

    let other = scene.fresh_files();
    assert_eq!(names(&other.list("/Archiv").await.unwrap()), ["Unterlagen"]);
    assert_eq!(
        names(&other.list("/Archiv/Unterlagen").await.unwrap()),
        ["Belege", "b.txt"]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn files_that_leave_encryption_for_an_ordinary_folder_are_counted_for_the_question() {
    let scene = filled(&tree(&[
        ("a.txt", "a"),
        ("Belege/b.txt", "b"),
        ("Belege/c.txt", "c"),
    ]))
    .await;
    scene.fake.insert(BUCKET, "Offen/", b"");
    let into_offen = || side(scene.files());
    let sources = ["/Unterlagen/a.txt", "/Unterlagen/Belege"];
    assert_eq!(
        plan(&scene, &sources, into_offen(), "/Offen")
            .await
            .leaving(),
        3
    );
    let (_dir, real) = device();
    let to_device = plan(&scene, &sources, Side::Device, &real.to_string_lossy()).await;
    assert_eq!(to_device.leaving(), 0, "to the device nothing is asked");
    let itself = plan(&scene, &["/Unterlagen"], into_offen(), "/Offen").await;
    assert_eq!(itself.leaving(), 0, "the folder itself stays encrypted");

    // The question before the start: a yes goes on and is not asked again on a retry.
    let asked = plan(&scene, &["/Unterlagen/a.txt"], into_offen(), "/Offen").await;
    let cancel = CancellationToken::new();
    let events = std::sync::Mutex::new(Vec::new());
    let send = |event| events.lock().unwrap().push(event);
    let (answers, answered) = mpsc::channel();
    answers
        .send((TransferAnswer::Name(ConflictChoice::Skip), false))
        .unwrap();
    answers
        .send((TransferAnswer::Leave(LeaveChoice::Cancel), false))
        .unwrap();
    assert!(!confirm_leaving(&asked, &cancel, &send, &answered));
    assert_eq!(asked.leaving(), 1);
    answers
        .send((TransferAnswer::Leave(LeaveChoice::Proceed), false))
        .unwrap();
    assert!(confirm_leaving(&asked, &cancel, &send, &answered));
    assert_eq!(asked.leaving(), 0);
    assert!(
        confirm_leaving(&asked, &cancel, &send, &answered),
        "asked once"
    );
    let question = TransferEvent::LeavesEncryption {
        leaves_encryption: Leaving { count: 1 },
    };
    assert_eq!(*events.lock().unwrap(), [question.clone(), question]);
    // A closed vault ends the wait.
    let again = plan(&scene, &["/Unterlagen/a.txt"], into_offen(), "/Offen").await;
    cancel.cancel();
    assert!(!confirm_leaving(&again, &cancel, &send, &answered));

    // After the yes the file lies there in the clear.
    let copied = within(
        &scene,
        TransferOp::Copy,
        &["/Unterlagen/a.txt"],
        "/Offen",
        None,
    )
    .await;
    assert_eq!(copied, Outcome::Done);
    assert_eq!(scene.fake.object(BUCKET, "Offen/a.txt").unwrap(), b"a");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_move_out_removes_what_it_took_and_the_emptied_folders() {
    let scene = filled(&tree(&[
        ("Belege/2026/strom.pdf", "pdf"),
        ("Belege/gas.txt", "gas"),
        ("notiz.txt", "bleibt"),
    ]))
    .await;
    let (_dir, real) = device();
    let moved = transfer(
        TransferOp::Move,
        side(scene.files()),
        &["/Unterlagen/Belege".to_owned()],
        Side::Device,
        &real.to_string_lossy(),
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(moved, Outcome::Done);
    assert_eq!(
        std::fs::read(real.join("Belege/2026/strom.pdf")).unwrap(),
        b"pdf"
    );
    assert_eq!(std::fs::read(real.join("Belege/gas.txt")).unwrap(), b"gas");
    let other = scene.fresh_files();
    assert_eq!(
        names(&other.list("/Unterlagen").await.unwrap()),
        ["notiz.txt"]
    );
    let objects: Vec<String> = scene
        .keys()
        .into_iter()
        .filter(|key| key.contains("/m/") || key.contains("/c/"))
        .collect();
    assert_eq!(objects.len(), 2, "notiz.txt only: {objects:?}");

    // The whole folder moved out: nothing of it stays at the provider.
    let (_dir2, real2) = device();
    let moved = transfer(
        TransferOp::Move,
        side(scene.files()),
        &["/Unterlagen".to_owned()],
        Side::Device,
        &real2.to_string_lossy(),
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(moved, Outcome::Done);
    assert_eq!(
        std::fs::read(real2.join("Unterlagen/notiz.txt")).unwrap(),
        b"bleibt"
    );
    assert!(scene.keys().is_empty(), "{:?}", scene.keys());
}

/// An ordinary folder "Privat" with `plain.txt` and the encrypted folder "Unterlagen" holding
/// `notiz.txt`.
async fn private_folder() -> Scene {
    let scene = Scene::new();
    scene.fake.insert(BUCKET, "Privat/", b"");
    scene.fake.insert(BUCKET, "Privat/plain.txt", b"offen");
    create_encrypted_folder(&scene.files(), "/Privat", "Unterlagen")
        .await
        .unwrap();
    let (_dir, real) = device();
    std::fs::write(real.join("notiz.txt"), "geheim").unwrap();
    let outcome = transfer(
        TransferOp::Copy,
        Side::Device,
        &[real.join("notiz.txt").to_string_lossy().into_owned()],
        side(scene.files()),
        "/Privat/Unterlagen",
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(outcome, Outcome::Done);
    scene
}

#[tokio::test(flavor = "multi_thread")]
async fn an_ordinary_folder_goes_to_the_device_with_its_encrypted_folder_decrypted() {
    let scene = private_folder().await;
    let (_dir, real) = device();
    let moved = transfer(
        TransferOp::Move,
        side(scene.files()),
        &["/Privat".to_owned()],
        Side::Device,
        &real.to_string_lossy(),
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(moved, Outcome::Done);
    assert_eq!(
        std::fs::read(real.join("Privat/plain.txt")).unwrap(),
        b"offen"
    );
    assert_eq!(
        std::fs::read(real.join("Privat/Unterlagen/notiz.txt")).unwrap(),
        b"geheim"
    );
    let names: Vec<String> = walkdir::WalkDir::new(&real)
        .into_iter()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        !names
            .iter()
            .any(|name| name.contains(".hxef") || name == "h"),
        "no object of the encrypted folder as a file: {names:?}"
    );
    assert!(scene.keys().is_empty(), "moved: {:?}", scene.keys());
}

#[tokio::test(flavor = "multi_thread")]
async fn between_storages_an_ordinary_folder_keeps_its_encrypted_folder_as_it_is() {
    let scene = private_folder().await;
    let copied = transfer(
        TransferOp::Copy,
        side(scene.files()),
        &["/Privat".to_owned()],
        side(scene.files_in("c", "s2")),
        "/",
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(copied, Outcome::Done);
    let there = scene.fake.keys("c");
    assert!(
        there.iter().any(|key| key.ends_with(".hxef/h")),
        "{there:?}"
    );
    assert!(!there.iter().any(|key| key.contains("notiz")));
    let other = scene.files_in("c", "s2");
    assert_eq!(
        names(&other.list("/Privat").await.unwrap()),
        ["Unterlagen", "plain.txt"]
    );
    let text = other
        .read_start("/Privat/Unterlagen/notiz.txt", 6, 100)
        .await
        .unwrap();
    assert_eq!(text, b"geheim");
}
