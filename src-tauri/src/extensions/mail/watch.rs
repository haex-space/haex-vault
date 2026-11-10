//! Watching a mailbox for new messages (spec 017, US11, FR-056, FR-058): `mail:new-messages
//! {accountId, mailboxName, newCount}` to the frames of the extension that watches.
//!
//! holzi keeps one connection per watch and waits with IMAP IDLE (RFC 2177): the server reports
//! a new message itself. IDLE is renewed every [`IDLE_RENEW`] (servers may end it after 30
//! minutes); a lost connection is opened again after a growing pause. A server without IDLE is
//! asked every `intervalSeconds` instead. Only messages that arrive after the start count. The
//! credentials come with the start call and stay in memory for as long as the watch runs.
//! A watch ends when it is stopped, replaced, with its extension's last frame, when the extension
//! is disabled or removed (also on another device), when its `poll` permission goes
//! ([`end_revoked`]), when the server refuses the login (a wrong password is not tried again and
//! again until the account is locked), and with the process (the vault's end).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde_json::json;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::connect::{imap_login, ImapSession};
use super::{check_mailbox, decision, MailError, ServerConfig};
use crate::extensions::bridge::dispatch::Emit;
use crate::extensions::bridge::events::emit_to_frames;
use crate::extensions::host::ExtensionHost;
use crate::extensions::permissions::{Action, Decision};
use crate::vault_gate::VaultDb;

/// The SDK's event for new messages.
pub const NEW_MESSAGES: &str = "mail:new-messages";

/// How long one IDLE runs before holzi renews it.
pub const IDLE_RENEW: Duration = Duration::from_secs(25 * 60);
/// The bounds of the polling interval of a server without IDLE (the SDK's `intervalSeconds`).
#[cfg(not(test))]
pub const MIN_INTERVAL: Duration = Duration::from_secs(30);
#[cfg(test)]
pub const MIN_INTERVAL: Duration = Duration::from_millis(200);
pub const MAX_INTERVAL: Duration = Duration::from_secs(3600);
/// Watches one extension may run at a time.
pub const MAX_WATCHES: usize = 16;
/// The pauses before opening a lost connection again.
const FIRST_RETRY: Duration = Duration::from_secs(5);
const LAST_RETRY: Duration = Duration::from_secs(300);
const WATCH_OPERATION_TIMEOUT: Duration = Duration::from_secs(60);

type Key = (Uuid, String, String);

/// One running watch: what ends it, and the server its permission names.
struct Watch {
    id: u64,
    stop: CancellationToken,
    host: String,
    port: u16,
}

/// The running watches of this process.
#[derive(Default)]
pub struct MailWatches {
    running: Mutex<HashMap<Key, Watch>>,
    next_id: AtomicU64,
}

impl MailWatches {
    /// Locks the registry while recovering from a poisoned mutex.
    fn lock(&self) -> MutexGuard<'_, HashMap<Key, Watch>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Ends the watch of `account`/`mailbox` of `extension_id`; `false` if there was none.
    pub fn stop(&self, extension_id: Uuid, account: &str, mailbox: &str) -> bool {
        let key = (extension_id, account.to_owned(), mailbox.to_owned());
        self.lock().remove(&key).map(|w| w.stop.cancel()).is_some()
    }

    /// Ends every watch of `extension_id`.
    pub fn end_all(&self, extension_id: Uuid) {
        self.end_where(|ext, _, _| ext == extension_id);
    }

    /// Ends every watch for which `end(extension, host, port)` holds.
    fn end_where(&self, end: impl Fn(Uuid, &str, u16) -> bool) {
        self.lock().retain(|(ext, _, _), watch| {
            if end(*ext, &watch.host, watch.port) {
                watch.stop.cancel();
                false
            } else {
                true
            }
        });
    }

    /// Takes the watch `key` out of the registry if it is still the one with `id` (not replaced).
    fn finish(&self, key: &Key, id: u64) {
        let mut running = self.lock();
        if running.get(key).is_some_and(|w| w.id == id) {
            running.remove(key);
        }
    }

    /// Registers a watch of `extension_id` that runs nothing, for tests of when watches end.
    #[cfg(test)]
    pub fn insert_for_test(&self, extension_id: Uuid) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.lock().insert(
            (extension_id, "test".into(), format!("box-{id}")),
            Watch {
                id,
                stop: CancellationToken::new(),
                host: "imap.example.org".into(),
                port: 993,
            },
        );
    }

    /// Counts the watches currently registered for an extension.
    pub fn count(&self, extension_id: Uuid) -> usize {
        self.lock()
            .keys()
            .filter(|(ext, _, _)| *ext == extension_id)
            .count()
    }
}

/// What a watch knows about its mailbox: UIDVALIDITY and the highest UID it has reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Seen {
    validity: u32,
    highest: u32,
}

/// Converts an async-IMAP protocol error into the mail boundary error.
fn imap_err(error: async_imap::error::Error) -> MailError {
    MailError::Imap(error.to_string())
}

/// Selects `mailbox`; a changed UIDVALIDITY starts counting anew.
async fn select(
    session: &mut ImapSession,
    mailbox: &str,
    seen: &mut Option<Seen>,
) -> Result<(), MailError> {
    let selected = session.select(mailbox).await.map_err(imap_err)?;
    let validity = selected.uid_validity.unwrap_or(0);
    if seen.is_none_or(|s| s.validity != validity) {
        let highest = selected.uid_next.unwrap_or(1).saturating_sub(1);
        *seen = Some(Seen { validity, highest });
    }
    Ok(())
}

/// The number of messages above the highest one reported, which it then moves to.
async fn new_messages(session: &mut ImapSession, seen: &mut Seen) -> Result<u32, MailError> {
    let uids: HashSet<u32> = session
        .uid_search(format!("UID {}:*", seen.highest.saturating_add(1)))
        .await
        .map_err(imap_err)?;
    let newer: Vec<u32> = uids.into_iter().filter(|u| *u > seen.highest).collect();
    if let Some(highest) = newer.iter().max() {
        seen.highest = *highest;
    }
    Ok(newer.len() as u32)
}

/// Where a new message is reported.
struct Report {
    host: Arc<ExtensionHost>,
    emitter: Arc<dyn Emit>,
    extension_id: Uuid,
    account: String,
    mailbox: String,
    /// The watch's entry in the registry.
    id: u64,
}

impl Report {
    /// Emits the SDK event when the polling/search operation found new messages.
    fn new_messages(&self, count: u32) {
        if count == 0 {
            return;
        }
        emit_to_frames(
            &*self.emitter,
            &self.host,
            self.extension_id,
            NEW_MESSAGES,
            &json!({ "accountId": self.account, "mailboxName": self.mailbox, "newCount": count }),
        );
    }
}

/// One connection's life: log in, select, then IDLE (or poll) until the connection fails or the
/// watch ends. `Ok` only when the watch ended.
async fn connected(
    config: &ServerConfig,
    report: &Report,
    interval: Duration,
    seen: &mut Option<Seen>,
    stop: &CancellationToken,
) -> Result<(), MailError> {
    let mut session = tokio::time::timeout(WATCH_OPERATION_TIMEOUT, imap_login(config))
        .await
        .map_err(|_| MailError::Timeout)??;
    let idle = tokio::time::timeout(WATCH_OPERATION_TIMEOUT, session.capabilities())
        .await
        .map_err(|_| MailError::Timeout)?
        .map_err(imap_err)?
        .has_str("IDLE");
    tokio::time::timeout(
        WATCH_OPERATION_TIMEOUT,
        select(&mut session, &report.mailbox, seen),
    )
    .await
    .map_err(|_| MailError::Timeout)??;
    loop {
        let mut current = seen.unwrap_or(Seen {
            validity: 0,
            highest: 0,
        });
        let count = tokio::time::timeout(
            WATCH_OPERATION_TIMEOUT,
            new_messages(&mut session, &mut current),
        )
        .await
        .map_err(|_| MailError::Timeout)??;
        report.new_messages(count);
        *seen = Some(current);
        if idle {
            let mut handle = session.idle();
            tokio::time::timeout(WATCH_OPERATION_TIMEOUT, handle.init())
                .await
                .map_err(|_| MailError::Timeout)?
                .map_err(imap_err)?;
            let (waiting, _interrupt) = handle.wait_with_timeout(IDLE_RENEW);
            tokio::select! {
                () = stop.cancelled() => return Ok(()),
                result = waiting => { result.map_err(imap_err)?; }
            }
            session = tokio::time::timeout(WATCH_OPERATION_TIMEOUT, handle.done())
                .await
                .map_err(|_| MailError::Timeout)?
                .map_err(imap_err)?;
        } else {
            tokio::select! {
                () = stop.cancelled() => return Ok(()),
                () = tokio::time::sleep(interval) => {}
            }
            // NOOP lets the server tell about new messages of the selected mailbox.
            tokio::time::timeout(WATCH_OPERATION_TIMEOUT, session.noop())
                .await
                .map_err(|_| MailError::Timeout)?
                .map_err(imap_err)?;
        }
    }
}

/// Reconnects a watch after failures until its cancellation token is triggered.
async fn run(config: ServerConfig, report: Report, interval: Duration, stop: CancellationToken) {
    let mut seen = None;
    let mut pause = FIRST_RETRY;
    loop {
        let outcome = tokio::select! {
            () = stop.cancelled() => return,
            outcome = connected(&config, &report, interval, &mut seen, &stop) => outcome,
        };
        match outcome {
            Ok(()) => return,
            Err(MailError::Auth) => {
                log::warn!(
                    "mail watch of an extension on {}: the server refused the login; it ends",
                    config.host
                );
                let key = (
                    report.extension_id,
                    report.account.clone(),
                    report.mailbox.clone(),
                );
                report.host.mail_watches.finish(&key, report.id);
                return;
            }
            Err(error) => log::warn!(
                "mail watch of an extension on {}: {error}; trying again",
                config.host
            ),
        }
        tokio::select! {
            () = stop.cancelled() => return,
            () = tokio::time::sleep(pause) => {}
        }
        pause = (pause * 2).min(LAST_RETRY);
    }
}

/// Starts (or replaces) the watch of `account`/`mailbox`; it runs on the current runtime.
pub fn start(
    host: &Arc<ExtensionHost>,
    emitter: &Arc<dyn Emit>,
    extension_id: Uuid,
    account: String,
    mailbox: String,
    config: ServerConfig,
    interval: Duration,
) -> Result<(), MailError> {
    check_mailbox(&mailbox)?;
    if account.is_empty() || account.len() > 256 {
        return Err(MailError::Invalid("accountId not allowed".into()));
    }
    super::check_server(&config.host, config.port, config.security)?;
    let watches = &host.mail_watches;
    let key = (extension_id, account.clone(), mailbox.clone());
    let stop = CancellationToken::new();
    let id = watches.next_id.fetch_add(1, Ordering::Relaxed);
    {
        let mut running = watches.lock();
        let replacing = running.contains_key(&key);
        if !replacing
            && running
                .keys()
                .filter(|(e, _, _)| *e == extension_id)
                .count()
                >= MAX_WATCHES
        {
            return Err(MailError::TooLarge);
        }
        let watch = Watch {
            id,
            stop: stop.clone(),
            host: config.host.to_ascii_lowercase(),
            port: config.port,
        };
        if let Some(old) = running.insert(key, watch) {
            old.stop.cancel();
        }
    }
    let report = Report {
        host: Arc::clone(host),
        emitter: Arc::clone(emitter),
        extension_id,
        account,
        mailbox,
        id,
    };
    let runtime = tokio::runtime::Handle::try_current()
        .map_err(|_| MailError::Invalid("no runtime for the watch".into()))?;
    runtime.spawn(run(
        config,
        report,
        interval.clamp(MIN_INTERVAL, MAX_INTERVAL),
        stop,
    ));
    Ok(())
}

/// Ends every watch whose `poll` permission for its server no longer allows it (FR-020: a revoked
/// permission applies at once). Called wherever the file watches are checked again
/// (`fs::end_revoked_watches`); a failed read of the permissions ends nothing.
pub fn end_revoked(db: &VaultDb, host: &ExtensionHost, device: Uuid) {
    let servers: Vec<(Uuid, String, u16)> = host
        .mail_watches
        .lock()
        .iter()
        .map(|((ext, _, _), w)| (*ext, w.host.clone(), w.port))
        .collect();
    let revoked: HashSet<(Uuid, String, u16)> = servers
        .into_iter()
        .filter(|(ext, server, port)| {
            matches!(
                decision(db, host, *ext, device, &Action::Poll, server, *port),
                Ok(Decision::Deny | Decision::Prompt)
            )
        })
        .collect();
    if revoked.is_empty() {
        return;
    }
    host.mail_watches
        .end_where(|ext, server, port| revoked.contains(&(ext, server.to_owned(), port)));
}
