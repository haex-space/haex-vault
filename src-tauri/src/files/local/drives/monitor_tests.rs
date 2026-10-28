use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::*;
use crate::files::local::drives::backend::fake::FakeBackend;
use crate::files::local::drives::{DriveKind, DriveRelease, DriveState};

const TICK: Duration = Duration::from_millis(10);

fn stick(id: &str, free: u64) -> Drive {
    Drive {
        id: id.to_owned(),
        key: None,
        label: Some(id.to_owned()),
        path: Some(format!("/media/{id}")),
        kind: DriveKind::Removable,
        state: DriveState::Mounted,
        release: DriveRelease::Eject,
        encrypted: false,
        size: Some(100),
        total: Some(100),
        free: Some(free),
    }
}

fn recorder() -> (Arc<Mutex<Vec<DrivesChanged>>>, DriveSink) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let into = Arc::clone(&seen);
    (
        seen,
        Box::new(move |message: &DrivesChanged| {
            into.lock().unwrap().push(message.clone());
            true
        }),
    )
}

fn wait_until(what: impl Fn() -> bool) {
    let start = Instant::now();
    while !what() {
        assert!(start.elapsed() < Duration::from_secs(5), "timed out");
        std::thread::sleep(TICK);
    }
}

#[test]
fn a_new_listener_gets_the_list_at_once() {
    let backend = Arc::new(FakeBackend::with(vec![stick("a", 50)]));
    let monitor = DriveMonitor::new(backend, Duration::from_secs(60));
    let (seen, sink) = recorder();
    monitor.subscribe(sink);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].drives[0].id, "a");
    drop(seen);
    monitor.unsubscribe_all();
}

#[test]
fn a_change_is_sent_once() {
    let backend = Arc::new(FakeBackend::with(vec![stick("a", 50)]));
    let monitor = DriveMonitor::new(backend.clone(), TICK);
    let (seen, sink) = recorder();
    monitor.subscribe(sink);
    backend.drives.lock().unwrap().push(stick("b", 10));
    wait_until(|| seen.lock().unwrap().len() >= 2);
    // A few more rounds without a change send nothing.
    let rounds = *backend.list_count.lock().unwrap();
    wait_until(|| *backend.list_count.lock().unwrap() >= rounds + 3);
    assert_eq!(seen.lock().unwrap().len(), 2);
    assert_eq!(seen.lock().unwrap()[1].drives.len(), 2);
    monitor.unsubscribe_all();
}

#[test]
fn free_space_alone_is_no_change() {
    assert!(!changed(&[stick("a", 50)], &[stick("a", 40)]));
    assert!(changed(&[stick("a", 50)], &[stick("b", 50)]));
    assert!(changed(&[stick("a", 50)], &[]));
    let mut unmounted = stick("a", 50);
    unmounted.state = DriveState::Unmounted;
    assert!(changed(&[stick("a", 50)], &[unmounted]));
}

#[test]
fn the_last_listener_leaving_stops_the_polling() {
    let backend = Arc::new(FakeBackend::with(vec![stick("a", 50)]));
    let monitor = DriveMonitor::new(backend.clone(), TICK);
    let (_seen, sink) = recorder();
    let id = monitor.subscribe(sink);
    wait_until(|| *backend.list_count.lock().unwrap() >= 3);
    monitor.unsubscribe(id);
    assert!(!monitor.is_running());
    std::thread::sleep(TICK * 3);
    let after = *backend.list_count.lock().unwrap();
    std::thread::sleep(TICK * 5);
    assert_eq!(*backend.list_count.lock().unwrap(), after);
}

#[test]
fn a_listener_that_is_gone_is_dropped() {
    let backend = Arc::new(FakeBackend::with(vec![stick("a", 50)]));
    let monitor = DriveMonitor::new(backend.clone(), TICK);
    let calls = Arc::new(Mutex::new(0));
    let counted = Arc::clone(&calls);
    monitor.subscribe(Box::new(move |_| {
        *counted.lock().unwrap() += 1;
        // Takes the first list, then reports the window closed.
        *counted.lock().unwrap() == 1
    }));
    backend.drives.lock().unwrap().push(stick("b", 1));
    wait_until(|| !monitor.is_running());
    assert_eq!(*calls.lock().unwrap(), 2);
}
