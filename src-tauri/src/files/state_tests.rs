use std::sync::mpsc;
use std::time::Duration;

use super::*;
use crate::files::local::watch::watch_folder;

/// Long enough for the debounce and a slow CI machine.
const WAIT: Duration = Duration::from_secs(10);

fn state() -> (tempfile::TempDir, FilesState) {
    let dir = tempfile::tempdir().unwrap();
    let state = FilesState::new(
        OwnPlaces::default(),
        Vec::new(),
        dir.path().join("thumbnails"),
    );
    (dir, state)
}

#[test]
fn a_reload_ends_every_watch() {
    let (dir, files) = state();
    let real = std::fs::canonicalize(dir.path()).unwrap();
    let (sender, receiver) = mpsc::channel();
    let first = files.keep_watch(
        watch_folder(&real, false, move |changes| {
            let _ = sender.send(changes);
        })
        .unwrap(),
    );
    let second = files.keep_watch(watch_folder(&real, false, |_| {}).unwrap());

    files.unwatch_all();

    std::fs::write(real.join("neu.txt"), "x").unwrap();
    assert!(receiver.recv_timeout(WAIT).is_err());
    assert!(!files.unwatch(first));
    assert!(!files.unwatch(second));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_close_drops_the_opened_encrypted_folders_their_thumbnails_and_copies() {
    let (_dir, files) = state();
    let gate = VaultGate::with_runtime(tokio::runtime::Handle::current());
    gate.begin_session().unwrap();
    files.encrypted.set_heads("s1", "", Vec::new());
    files
        .memory_thumbnails
        .put("k".to_owned(), &Ok(vec![1, 2, 3]));
    let copy = files.opened_encrypted.join("a").join("brief.pdf");
    std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
    std::fs::write(&copy, "entschlüsselt").unwrap();
    files.forget_on_close(&gate);
    files.forget_on_close(&gate);
    assert!(files.encrypted.heads("s1", "").is_some());

    gate.request_close();
    tokio::time::timeout(WAIT, async {
        while files.encrypted.heads("s1", "").is_some()
            || files.memory_thumbnails.size() > 0
            || files.opened_encrypted.exists()
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("forgotten after the close");
    assert!(
        !files.thumbnails.starts_with(&files.opened_encrypted),
        "a place of its own"
    );
}
