//! Background IMAP polling for "new mail arrived" push events.
//!
//! Not IMAP IDLE: `crate::mail` connects fresh per call (see `mail/mod.rs`
//! doc comment) with no persistent session, so this instead runs a per
//! `(extension, account, mailbox)` timer that does a cheap `STATUS`
//! round-trip (UIDVALIDITY/UIDNEXT only, no envelope fetch) and emits a
//! Tauri event to the owning extension when the high-water mark advances.
//!
//! Credentials are resolved directly from the vault DB (same trust level
//! as `passwords::commands::extension_password_read`) rather than being
//! handed a plaintext `ImapConfig` by the frontend — the poll loop runs
//! detached from any single request, so there is no per-call caller to
//! hand it credentials each tick.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value as JsonValue;
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex as AsyncMutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::database::core::select_with_crdt;
use crate::database::error::DatabaseError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{MailAction, Principal};
use crate::extension::utils::get_extension_table_prefix;
use crate::mail::types::{ConnectionSecurity, ImapConfig};
use crate::AppState;

/// Protects mail servers from a misconfigured/absurdly low interval, and
/// keeps a "watch" meaningfully live rather than degrading into a no-op.
const MIN_POLL_INTERVAL_SECS: u64 = 30;
const MAX_POLL_INTERVAL_SECS: u64 = 3600;

/// Upper bound on a single STATUS round-trip. Without this, a stalled IMAP
/// server can hold `close_database`'s await-all-handles shutdown open
/// indefinitely (see `run_poll_loop`).
const STATUS_CHECK_TIMEOUT: Duration = Duration::from_secs(30);

pub const MAIL_NEW_MESSAGES_EVENT: &str = "mail:new-messages";

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MailNewMessagesEvent {
    pub account_id: String,
    pub mailbox_name: String,
    pub new_count: u32,
}

pub fn mail_poll_key(extension_id: &str, account_id: &str, mailbox_name: &str) -> String {
    format!("{extension_id}::{account_id}::{mailbox_name}")
}

/// Tracks running per-`(extension, account, mailbox)` poll tasks and the
/// UID baseline used to detect newly-arrived mail.
pub struct MailPollManager {
    active: HashMap<String, (CancellationToken, JoinHandle<()>)>,
    /// key -> (uid_validity, last_seen_uid). Seeded on the first tick after
    /// a watch starts so pre-existing mail is never reported as "new" —
    /// only mail arriving after the watch began triggers an event.
    baselines: HashMap<String, (u32, u32)>,
    /// Per-key lock serializing the start/stop lifecycle (take-old-handle →
    /// await → spawn → register) for a single watch. Without it, two
    /// concurrent `extension_mail_start_watch`/`extension_mail_stop_watch`
    /// calls for the same key can interleave across that gap: one start can
    /// overwrite another's handle (orphaning a task), or a stop can return
    /// successfully before a concurrent start has registered its task.
    lifecycle_locks: HashMap<String, Arc<AsyncMutex<()>>>,
}

impl MailPollManager {
    pub fn new() -> Self {
        Self {
            active: HashMap::new(),
            baselines: HashMap::new(),
            lifecycle_locks: HashMap::new(),
        }
    }

    /// Fetch (or create) the lifecycle lock for `key`. Callers must acquire
    /// this lock BEFORE reading/mutating `active`/`baselines` for the key,
    /// and hold it for the entire start-or-stop sequence.
    pub fn lifecycle_lock(&mut self, key: &str) -> Arc<AsyncMutex<()>> {
        self.lifecycle_locks
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone()
    }

    pub fn is_running(&self, key: &str) -> bool {
        self.active.contains_key(key)
    }

    /// Cancel + remove a watch, returning its `JoinHandle` for the caller to
    /// await OUTSIDE the manager lock — same deadlock-avoidance rationale as
    /// `file_sync::SyncManager::take_stop`.
    pub fn take_stop(&mut self, key: &str) -> Option<JoinHandle<()>> {
        self.baselines.remove(key);
        self.active.remove(key).map(|(token, handle)| {
            token.cancel();
            handle
        })
    }

    pub fn take_stop_all(&mut self) -> Vec<(String, JoinHandle<()>)> {
        self.baselines.clear();
        self.active
            .drain()
            .map(|(key, (token, handle))| {
                token.cancel();
                (key, handle)
            })
            .collect()
    }

    /// Remove a watch without awaiting its `JoinHandle` — used by the poll
    /// loop itself when it detects its account no longer exists, so it can
    /// exit without deadlocking on its own handle (mirrors
    /// `SyncManager::deregister`).
    pub fn deregister(&mut self, key: &str) {
        self.baselines.remove(key);
        if let Some((token, _handle)) = self.active.remove(key) {
            token.cancel();
        }
    }

    pub fn register(&mut self, key: String, token: CancellationToken, handle: JoinHandle<()>) {
        self.active.insert(key, (token, handle));
    }

    fn get_baseline(&self, key: &str) -> Option<(u32, u32)> {
        self.baselines.get(key).copied()
    }

    fn set_baseline(&mut self, key: &str, uid_validity: u32, last_seen_uid: u32) {
        self.baselines
            .insert(key.to_string(), (uid_validity, last_seen_uid));
    }
}

impl Default for MailPollManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolves an `ImapConfig` for `account_id` by reading haex-mail's own
/// `accounts` table (extension-scoped, prefixed `{public_key}__{name}__`)
/// for host/port/security + the linked password-vault item id, then
/// reading username/password directly from the core passwords table. This
/// mirrors what haex-mail's frontend does via `loadAccountWithCredentialsAsync`
/// (apps/haex-mail/app/stores/accounts.ts) but runs entirely in Rust so the
/// poll loop never needs the frontend to hand it a plaintext credential.
pub(crate) async fn resolve_account_imap_config(
    state: &AppState,
    public_key: &str,
    extension_name: &str,
    account_id: &str,
) -> Result<Option<ImapConfig>, DatabaseError> {
    let prefix = get_extension_table_prefix(public_key, extension_name);
    let accounts_sql = format!(
        "SELECT imap_host, imap_port, imap_security, password_item_id FROM {prefix}accounts WHERE id = ?1"
    );
    let account_rows = select_with_crdt(
        accounts_sql,
        vec![JsonValue::String(account_id.to_string())],
        &state.db,
    )?;
    let Some(account_row) = account_rows.first() else {
        return Ok(None);
    };

    let host = account_row[0].as_str().unwrap_or_default().to_string();
    let port = account_row[1].as_u64().unwrap_or(993) as u16;
    let security = match account_row[2].as_str().unwrap_or("tls") {
        "startTls" => ConnectionSecurity::StartTls,
        "none" => ConnectionSecurity::None,
        _ => ConnectionSecurity::Tls,
    };
    let password_item_id = account_row[3].as_str().unwrap_or_default().to_string();

    let password_rows = select_with_crdt(
        "SELECT username, password FROM haex_passwords_item_details WHERE id = ?1".to_string(),
        vec![JsonValue::String(password_item_id)],
        &state.db,
    )?;
    let Some(password_row) = password_rows.first() else {
        return Ok(None);
    };
    let username = password_row[0].as_str().unwrap_or_default().to_string();
    let password = password_row[1].as_str().unwrap_or_default().to_string();

    Ok(Some(ImapConfig {
        host,
        port,
        security,
        username,
        password,
    }))
}

/// Runs until `cancel` fires. Every tick: resolve credentials, run a cheap
/// `STATUS` check (UIDVALIDITY/UIDNEXT, no envelope fetch), and emit
/// `MAIL_NEW_MESSAGES_EVENT` if the high-water mark advanced.
#[allow(clippy::too_many_arguments)]
pub async fn run_poll_loop(
    app_handle: AppHandle,
    extension_id: String,
    public_key: String,
    extension_name: String,
    account_id: String,
    mailbox_name: String,
    interval_seconds: u64,
    cancel: CancellationToken,
) {
    let key = mail_poll_key(&extension_id, &account_id, &mailbox_name);
    let interval = interval_seconds.clamp(MIN_POLL_INTERVAL_SECS, MAX_POLL_INTERVAL_SECS);
    let mut ticker = tokio::time::interval(Duration::from_secs(interval));

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = ticker.tick() => {
                let Some(state) = app_handle.try_state::<AppState>() else {
                    continue;
                };

                let imap_config = match resolve_account_imap_config(
                    &state,
                    &public_key,
                    &extension_name,
                    &account_id,
                )
                .await
                {
                    Ok(Some(config)) => config,
                    Ok(None) => {
                        // Account was deleted — stop watching it. `deregister`
                        // (not `take_stop`) because we're running inside the
                        // very task the manager would otherwise try to await.
                        state.mail_poll_manager.lock().await.deregister(&key);
                        break;
                    }
                    Err(DatabaseError::ConnectionError { .. }) => {
                        // Vault is locked — skip this tick silently, retry next interval.
                        continue;
                    }
                    Err(e) => {
                        eprintln!("[mail-poll] failed to resolve credentials for {key}: {e}");
                        continue;
                    }
                };

                // Permission was checked once in `extension_mail_start_watch`,
                // against the host that was current at start time. Recheck it
                // every tick against the freshly-resolved host: the account row
                // can change afterward, and a revoked `Poll` grant must stop the
                // watch rather than keep sending credentials in the background.
                if PermissionManager::check_mail_permission(
                    &state,
                    &Principal::Extension(extension_id.clone()),
                    MailAction::Poll,
                    &imap_config.host,
                )
                .await
                .is_err()
                {
                    state.mail_poll_manager.lock().await.deregister(&key);
                    break;
                }

                // Race the STATUS round-trip against cancellation and a bounded
                // timeout — `close_database` awaits this task's handle on vault
                // lock, so a stalled IMAP server must not hold shutdown open.
                let status_check = crate::mail::imap::list_mailboxes(
                    &imap_config,
                    None,
                    Some(&mailbox_name),
                    true,
                );
                let mailboxes = tokio::select! {
                    _ = cancel.cancelled() => break,
                    result = tokio::time::timeout(STATUS_CHECK_TIMEOUT, status_check) => {
                        match result {
                            Ok(Ok(mailboxes)) => mailboxes,
                            Ok(Err(e)) => {
                                eprintln!("[mail-poll] IMAP status check failed for {key}: {e}");
                                continue;
                            }
                            Err(_) => {
                                eprintln!("[mail-poll] IMAP status check timed out for {key}");
                                continue;
                            }
                        }
                    }
                };

                let Some(mailbox) = mailboxes.into_iter().find(|m| m.name == mailbox_name) else {
                    eprintln!("[mail-poll] mailbox {mailbox_name} not found for {key}");
                    continue;
                };
                let (Some(uid_validity), Some(uid_next)) = (mailbox.uid_validity, mailbox.uid_next)
                else {
                    eprintln!("[mail-poll] server did not report UIDVALIDITY/UIDNEXT for {key}");
                    continue;
                };

                let baseline = state.mail_poll_manager.lock().await.get_baseline(&key);
                let new_count = match baseline {
                    None => {
                        // First tick: seed the baseline at the current
                        // high-water mark WITHOUT emitting — otherwise every
                        // pre-existing message would be reported as "new".
                        state
                            .mail_poll_manager
                            .lock()
                            .await
                            .set_baseline(&key, uid_validity, uid_next.saturating_sub(1));
                        None
                    }
                    Some((prev_uid_validity, _)) if prev_uid_validity != uid_validity => {
                        // Mailbox was recreated (UIDVALIDITY changed) — old
                        // UIDs are meaningless now. Reseed rather than diff.
                        state
                            .mail_poll_manager
                            .lock()
                            .await
                            .set_baseline(&key, uid_validity, uid_next.saturating_sub(1));
                        None
                    }
                    Some((_, prev_last_seen)) if uid_next > prev_last_seen + 1 => {
                        // UIDNEXT can advance without `prev_last_seen`
                        // messages actually landing — servers may leave gaps
                        // in the UID sequence — so diffing UIDNEXT alone can
                        // over-report `new_count`. Count the UIDs that
                        // actually exist above the baseline instead.
                        // Same cancellation/timeout guard as the STATUS check:
                        // `count_new_uids` opens a fresh IMAP connection, so a
                        // stalled server must not hold `close_database` open on
                        // vault lock.
                        let count_check = crate::mail::imap::count_new_uids(
                            &imap_config,
                            &mailbox_name,
                            prev_last_seen,
                        );
                        let count = tokio::select! {
                            _ = cancel.cancelled() => break,
                            result = tokio::time::timeout(STATUS_CHECK_TIMEOUT, count_check) => result,
                        };
                        match count {
                            Ok(Ok(count)) => {
                                state.mail_poll_manager.lock().await.set_baseline(
                                    &key,
                                    uid_validity,
                                    uid_next.saturating_sub(1),
                                );
                                Some(count)
                            }
                            Ok(Err(e)) => {
                                // Leave the baseline unchanged so the next tick
                                // retries the count from the same starting point.
                                eprintln!(
                                    "[mail-poll] failed to count new UIDs for {key}: {e}"
                                );
                                None
                            }
                            Err(_) => {
                                eprintln!(
                                    "[mail-poll] counting new UIDs timed out for {key}"
                                );
                                None
                            }
                        }
                    }
                    Some(_) => None,
                };

                if let Some(new_count) = new_count {
                    let payload = MailNewMessagesEvent {
                        account_id: account_id.clone(),
                        mailbox_name: mailbox_name.clone(),
                        new_count,
                    };
                    // `extension_webview_manager` isn't part of `AppState` on
                    // mobile (extensions run as iframes in the main window
                    // there), so mirror its two platform impls at the call
                    // site instead of depending on the missing field.
                    #[cfg(not(any(target_os = "android", target_os = "ios")))]
                    {
                        let _ = state.extension_webview_manager.emit_to_extension_or_main(
                            &app_handle,
                            &extension_id,
                            MAIL_NEW_MESSAGES_EVENT,
                            payload,
                        );
                    }
                    #[cfg(any(target_os = "android", target_os = "ios"))]
                    {
                        let _ = app_handle.emit_to("main", MAIL_NEW_MESSAGES_EVENT, payload);
                    }
                }
            }
        }
    }
}
