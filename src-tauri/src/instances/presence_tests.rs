use std::sync::mpsc;
use std::time::Duration;

use super::ProcessPresence;

#[test]
fn the_first_announce_runs_its_callback_while_alone() {
    let dir = tempfile::tempdir().expect("dir");
    let mut ran = false;
    let _presence = ProcessPresence::announce(dir.path(), || ran = true).expect("announce");
    assert!(ran);
}

#[test]
fn only_the_process_that_started_alone_reports_it() {
    let dir = tempfile::tempdir().expect("dir");
    let first = ProcessPresence::announce(dir.path(), || {}).expect("first announce");
    let second = ProcessPresence::announce(dir.path(), || {}).expect("second announce");
    assert!(first.was_alone());
    assert!(!second.was_alone());
}

#[test]
fn a_second_announce_while_the_first_is_alive_does_not_run_its_callback() {
    let dir = tempfile::tempdir().expect("dir");
    let _first = ProcessPresence::announce(dir.path(), || {}).expect("first announce");

    let mut ran = false;
    let _second = ProcessPresence::announce(dir.path(), || ran = true).expect("second announce");
    assert!(!ran, "a second, non-alone announce must skip its callback");
}

#[test]
fn a_new_announce_runs_its_callback_again_once_the_first_drops() {
    let dir = tempfile::tempdir().expect("dir");
    let first = ProcessPresence::announce(dir.path(), || {}).expect("first announce");
    drop(first);

    let mut ran = false;
    let _second = ProcessPresence::announce(dir.path(), || ran = true).expect("second announce");
    assert!(
        ran,
        "the lock is fully released once the first handle drops"
    );
}

/// While the first `announce`'s callback is still running (holding the exclusive lock), a second
/// `announce` on another thread must not return; once the first callback finishes, the second
/// returns without ever having run its own callback. Coordinated with channels, not sleeps,
/// matching `vault_gate::drain_tests`'s own idiom for the same class of proof.
#[test]
fn a_second_announce_on_another_thread_blocks_until_the_first_callback_finishes_then_skips_its_own()
{
    let dir = tempfile::tempdir().expect("dir");
    let dir_path = dir.path().to_path_buf();

    let (first_running_tx, first_running_rx) = mpsc::channel::<()>();
    let (release_first_tx, release_first_rx) = mpsc::channel::<()>();
    let first_thread = std::thread::spawn({
        let dir_path = dir_path.clone();
        move || {
            ProcessPresence::announce(&dir_path, || {
                first_running_tx.send(()).expect("report running");
                release_first_rx.recv().expect("wait to be released");
            })
            .expect("first announce")
        }
    });
    first_running_rx.recv().expect("first callback is running");

    let (second_ran_tx, second_ran_rx) = mpsc::channel::<()>();
    let (second_returned_tx, second_returned_rx) = mpsc::channel::<()>();
    let second_thread = std::thread::spawn(move || {
        let presence = ProcessPresence::announce(&dir_path, || {
            second_ran_tx
                .send(())
                .expect("report running (must never happen)");
        })
        .expect("second announce");
        second_returned_tx.send(()).expect("report returned");
        presence
    });

    assert!(
        second_returned_rx.try_recv().is_err(),
        "the second announce must not return while the first callback still holds the lock"
    );

    release_first_tx
        .send(())
        .expect("release the first callback");
    let _first_presence = first_thread.join().expect("first thread joins");

    second_returned_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("the second announce returns once the first finishes and downgrades");
    assert!(
        second_ran_rx.try_recv().is_err(),
        "the second announce must never run its own callback"
    );

    let _second_presence = second_thread.join().expect("second thread joins");
}
