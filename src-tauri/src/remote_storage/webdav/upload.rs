//! Uploads in parts over WebDAV (spec 050 research R7): one `PUT` of a temporary file in the target
//! folder whose body is fed part by part, then a `MOVE` onto the name. WebDAV cannot add to a file,
//! so a part that failed cannot be sent again: the session is used up and every later call is
//! [`StorageError::Network`], and the transfer starts over. A cancel ends the `PUT` and deletes the
//! temporary file, so nothing half stays under the name (FR-016).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::channel::mpsc;
use futures::SinkExt;
use reqwest::{Method, StatusCode};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use uuid::Uuid;

use super::super::address::Resolver;
use super::super::{Access, StorageError};
use super::request::{send_raw, status_error, url_of};

/// A session without a part for this long is dropped.
const IDLE: Duration = Duration::from_secs(30 * 60);

/// How long the `PUT` of a session may run in all; each part has its own deadline.
const WHOLE_UPLOAD: Duration = Duration::from_secs(24 * 60 * 60);

type Chunk = Result<Vec<u8>, std::io::Error>;

/// One upload in parts.
struct Session {
    /// The key of the temporary file.
    part: String,
    sender: Option<mpsc::Sender<Chunk>>,
    put: JoinHandle<Result<StatusCode, StorageError>>,
    touched: Instant,
    /// A part failed; the body of the `PUT` is broken.
    broken: bool,
}

/// The open uploads of one store, by id.
#[derive(Default)]
pub(super) struct Sessions {
    open: Mutex<HashMap<String, Session>>,
}

impl Sessions {
    fn take(&self, id: &str) -> Option<Session> {
        self.open.lock().expect("sessions").remove(id)
    }

    /// Ends sessions without a part for [`IDLE`]; their temporary files stay until a listing
    /// cleans them (research R7).
    fn drop_idle(&self) {
        let now = Instant::now();
        self.open.lock().expect("sessions").retain(|_, session| {
            let alive = now.duration_since(session.touched) < IDLE;
            if !alive {
                session.put.abort();
            }
            alive
        });
    }

    /// Starts the `PUT` of the temporary file `part` (in the folder of the target, which exists)
    /// and returns the id of the session.
    pub(super) fn begin(
        &self,
        resolver: Arc<dyn Resolver>,
        access: &Access,
        part: String,
    ) -> Result<String, StorageError> {
        self.drop_idle();
        let url = url_of(&access.location, &part)?;
        let (sender, receiver) = mpsc::channel::<Chunk>(0);
        let access = access.clone();
        let put = tokio::spawn(async move {
            let response = send_raw(
                resolver.as_ref(),
                &access,
                Method::PUT,
                url,
                &[],
                Some(reqwest::Body::wrap_stream(receiver)),
                Instant::now() + WHOLE_UPLOAD,
            )
            .await?;
            Ok(response.status())
        });
        let id = Uuid::new_v4().to_string();
        self.open.lock().expect("sessions").insert(
            id.clone(),
            Session {
                part,
                sender: Some(sender),
                put,
                touched: Instant::now(),
                broken: false,
            },
        );
        Ok(id)
    }

    /// Sends `body` into the `PUT` of session `id` before `deadline`.
    pub(super) async fn part(
        &self,
        id: &str,
        body: Vec<u8>,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        let mut sender = {
            let mut open = self.open.lock().expect("sessions");
            let session = open.get_mut(id).ok_or(StorageError::NotFound)?;
            if session.broken {
                return Err(StorageError::Network);
            }
            session.touched = Instant::now();
            session.sender.clone().ok_or(StorageError::Network)?
        };
        let sent = match tokio::time::timeout_at(deadline, sender.send(Ok(body))).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(StorageError::Network),
            Err(_) => Err(StorageError::TimedOut),
        };
        if sent.is_err() {
            if let Some(session) = self.open.lock().expect("sessions").get_mut(id) {
                session.broken = true;
            }
        }
        sent
    }

    /// Ends the body of session `id` and waits for the server to take the temporary file. Returns
    /// the key of the temporary file with the outcome, so the caller moves it on success and
    /// deletes it otherwise; `None` for an unknown session.
    pub(super) async fn finish(
        &self,
        id: &str,
        deadline: Instant,
    ) -> Option<(String, Result<(), StorageError>)> {
        let mut session = self.take(id)?;
        if session.broken {
            session.put.abort();
            return Some((session.part, Err(StorageError::Network)));
        }
        drop(session.sender.take());
        let outcome = match tokio::time::timeout_at(deadline, &mut session.put).await {
            Ok(Ok(Ok(status))) if status.is_success() => Ok(()),
            Ok(Ok(Ok(status))) => Err(status_error(&Method::PUT, status)),
            Ok(Ok(Err(error))) => Err(error),
            Ok(Err(_)) => Err(StorageError::Network),
            Err(_) => {
                session.put.abort();
                Err(StorageError::TimedOut)
            }
        };
        Some((session.part, outcome))
    }

    /// Ends session `id` without a file; returns the key of its temporary file to delete.
    pub(super) fn abort(&self, id: &str) -> Option<String> {
        let session = self.take(id)?;
        session.put.abort();
        Some(session.part)
    }
}
