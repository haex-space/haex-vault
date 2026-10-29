use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use tauri::ipc::{Channel, InvokeResponseBody};

use super::*;
use crate::remote_storage::test_support::{access, FakeStore};

const STICK: &str = "/media/u/STICK";
const WAIT: Duration = Duration::from_secs(10);

fn storage() -> Side {
    Side::Storage(Arc::new(StorageFiles::new(
        Arc::new(FakeStore::new()),
        access("b"),
    )))
}

fn remote(from: Side, sources: &[&str], to: Side, target: &str) -> RemoteJob {
    RemoteJob {
        op: TransferOp::Copy,
        from,
        sources: sources.iter().map(|path| (*path).to_owned()).collect(),
        to,
        target: target.to_owned(),
        known_free: false,
    }
}

#[test]
fn a_local_copy_into_the_drive_touches_it() {
    let job = Job {
        op: TransferOp::Copy,
        sources: vec![PathBuf::from("/home/u/a.txt")],
        target: Some(PathBuf::from("/media/u/STICK/fotos")),
    };
    assert!(local_touches(&job, Path::new(STICK)));
    assert!(!local_touches(&job, Path::new("/media/u/STICKER")));
}

#[test]
fn only_the_device_ends_of_a_remote_job_count() {
    let up = remote(Side::Device, &["/media/u/STICK/a.txt"], storage(), "/");
    assert!(remote_touches(&up, Path::new(STICK)));
    let down = remote(storage(), &["/a.txt"], Side::Device, "/home/u");
    assert!(!remote_touches(&down, Path::new(STICK)));
    // A storage's window path that looks like the drive is not on this device.
    let between = remote(storage(), &["/media/u/STICK/a.txt"], storage(), STICK);
    assert!(!remote_touches(&between, Path::new(STICK)));
}

/// A copy that waits on a taken name until it is answered or cancelled; the receiver hears every
/// event as JSON.
fn waiting_copy(
    manager: &TransferManager,
    gate: &VaultGate,
    from: &Path,
    to: &Path,
) -> (String, mpsc::Receiver<String>) {
    std::fs::write(from.join("a.txt"), "neu").unwrap();
    std::fs::write(to.join("a.txt"), "alt").unwrap();
    let (sender, events) = mpsc::channel();
    let channel = Channel::new(move |body| {
        if let InvokeResponseBody::Json(json) = body {
            let _ = sender.send(json);
        }
        Ok(())
    });
    let job = Job {
        op: TransferOp::Copy,
        sources: vec![from.join("a.txt")],
        target: Some(to.to_path_buf()),
    };
    let totals = Totals { items: 1, bytes: 3 };
    let id = manager
        .start(gate, job, totals, Removal::Permanent, channel)
        .unwrap();
    loop {
        let event = events.recv_timeout(WAIT).expect("the conflict");
        if event.contains("\"conflict\"") {
            break;
        }
    }
    (id, events)
}

#[tokio::test(flavor = "multi_thread")]
async fn releasing_a_drive_cancels_its_transfers_and_waits_for_them() {
    let (home, stick) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let gate = VaultGate::with_runtime(tokio::runtime::Handle::current());
    gate.begin_session().unwrap();
    let manager = TransferManager::default();
    let (id, events) = waiting_copy(&manager, &gate, home.path(), stick.path());

    assert_eq!(manager.touching(stick.path()), vec![id.clone()]);
    assert!(manager
        .touching(home.path().join("other").as_path())
        .is_empty());

    assert_eq!(manager.cancel_touching(stick.path()).await, 1);

    // The worker has ended before the call returned: it said so and is forgotten.
    let said: Vec<String> = events.try_iter().collect();
    assert!(
        said.iter().any(|event| event.contains("\"cancelled\"")),
        "{said:?}"
    );
    assert!(manager.known().get(&id).is_none());
    assert!(manager.touching(stick.path()).is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_transfer_stays_known_until_its_worker_ends() {
    let (home, stick) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let gate = VaultGate::with_runtime(tokio::runtime::Handle::current());
    gate.begin_session().unwrap();
    let manager = TransferManager::default();
    let (id, events) = waiting_copy(&manager, &gate, home.path(), stick.path());

    manager.cancel(&id);
    // A cancel from the window leaves it known while it runs, so a release still waits for it.
    manager.cancel_touching(stick.path()).await;

    let said: Vec<String> = events.try_iter().collect();
    assert!(
        said.iter().any(|event| event.contains("\"cancelled\"")),
        "{said:?}"
    );
    assert!(manager.known().get(&id).is_none());
}
