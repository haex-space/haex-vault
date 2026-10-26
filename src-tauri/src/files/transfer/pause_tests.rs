use std::sync::mpsc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::pause::Pause;

const LONG: Duration = Duration::from_millis(300);

/// Runs `wait` on a thread; the receiver hears when it returned.
fn waiting(pause: &Pause, cancel: &CancellationToken) -> mpsc::Receiver<()> {
    let (done, returned) = mpsc::channel();
    let (pause, cancel) = (pause.clone(), cancel.clone());
    std::thread::spawn(move || {
        pause.wait(&cancel);
        let _ = done.send(());
    });
    returned
}

#[test]
fn a_transfer_that_is_not_paused_does_not_wait() {
    let returned = waiting(&Pause::default(), &CancellationToken::new());
    assert!(returned.recv_timeout(LONG).is_ok());
}

#[test]
fn a_paused_transfer_waits_until_it_goes_on() {
    let pause = Pause::default();
    pause.set(true);
    let returned = waiting(&pause, &CancellationToken::new());
    assert!(returned.recv_timeout(LONG).is_err());
    pause.set(false);
    assert!(returned.recv_timeout(LONG).is_ok());
}

#[test]
fn a_cancel_ends_the_pause() {
    let pause = Pause::default();
    pause.set(true);
    let cancel = CancellationToken::new();
    let returned = waiting(&pause, &cancel);
    assert!(returned.recv_timeout(LONG).is_err());
    cancel.cancel();
    assert!(returned.recv_timeout(LONG).is_ok());
}
