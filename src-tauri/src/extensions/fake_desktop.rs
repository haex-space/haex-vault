//! The system's notifications as tests see them (spec 017 US8, spec 057 T034): what holzi asked the
//! system to show, and a way to answer as the system would.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::host::Desktop;
use super::notifications::{NotificationResponse, NotificationSpec, Respond, ShownNotification};

/// Shows nothing; keeps what was asked and lets the test answer as the system would.
pub(crate) struct FakeDesktop {
    pub(crate) shown: Mutex<Vec<(NotificationSpec, Option<Respond>)>>,
    closed: Arc<Mutex<Vec<usize>>>,
    /// The person refused the system's notification permission.
    pub(crate) refused: AtomicBool,
}

impl Default for FakeDesktop {
    fn default() -> Self {
        Self {
            shown: Mutex::default(),
            closed: Arc::default(),
            refused: AtomicBool::new(false),
        }
    }
}

struct FakeShown {
    index: usize,
    closed: Arc<Mutex<Vec<usize>>>,
}

impl ShownNotification for FakeShown {
    fn close(self: Box<Self>) {
        self.closed.lock().unwrap().push(self.index);
    }
}

impl Desktop for FakeDesktop {
    fn open_url(&self, _url: &str) -> Result<(), String> {
        Ok(())
    }

    fn notifications_allowed(&self) -> bool {
        !self.refused.load(Ordering::Relaxed)
    }

    fn show_notification(
        &self,
        notification: &NotificationSpec,
        respond: Respond,
    ) -> Result<Box<dyn ShownNotification>, String> {
        let mut shown = self.shown.lock().unwrap();
        shown.push((notification.clone(), Some(respond)));
        let index = shown.len() - 1;
        Ok(Box::new(FakeShown {
            index,
            closed: Arc::clone(&self.closed),
        }))
    }
}

impl FakeDesktop {
    pub(crate) fn answer(&self, index: usize, response: NotificationResponse) {
        let respond = self.shown.lock().unwrap()[index].1.take().unwrap();
        respond(response);
    }

    pub(crate) fn count(&self) -> usize {
        self.shown.lock().unwrap().len()
    }

    /// The indexes of the notifications holzi took away again, in order.
    pub(crate) fn closed(&self) -> Vec<usize> {
        self.closed.lock().unwrap().clone()
    }

    /// What was asked to be shown, in order.
    pub(crate) fn specs(&self) -> Vec<NotificationSpec> {
        self.shown
            .lock()
            .unwrap()
            .iter()
            .map(|(spec, _)| spec.clone())
            .collect()
    }

    /// The system refuses to show notifications.
    pub(crate) fn forbid(&self) {
        self.refused.store(true, Ordering::Relaxed);
    }
}
