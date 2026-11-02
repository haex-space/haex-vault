//! Moving chat files (spec 072 US3, FR-031 to FR-037, research R11–R14). Sending: every file is
//! encrypted and uploaded to the vault's Blossom-Server before the message is queued, and this
//! device keeps its own copy. Receiving: every device downloads a file itself, by itself up to its
//! limit (FR-036), on request above it. A download that was cut off (vault closed, app ended) starts
//! again when the vault opens next (FR-037). Where a file stands on this device is device-local
//! (`chat_attachment_files_no_sync`).

use std::sync::Arc;
use std::time::Duration;

use haex_crdt::rusqlite::params;
use nostr::key::Keys;
use serde::Serialize;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::blossom::{self, BlossomError, Server};
use super::crypto::{self, FileKey};
use super::store::ChatFileStore;
use crate::chat::peer::delivery::now_ms;
use crate::chat::peer::envelope::{Attachment, BlobRef, FileAttachment};
use crate::chat::peer::session::Me;
use crate::chat::peer::{ids, send, settings, targets};
use crate::error::{HolziError, Result};
use crate::remote_storage::address::Resolver;
use crate::storage::query::Query;
use crate::sync::keys::hex;
use crate::vault_gate::VaultDb;

/// Due downloads are looked for this often even without a new message.
const LOOK_AGAIN: Duration = Duration::from_secs(10 * 60);
/// Progress of one transfer is reported at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// The event the window hears transfers by (`chat-transfer`).
pub const TRANSFER_EVENT: &str = "chat-transfer";

/// Where a transfer of an attachment stands, for the window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub attachment_id: String,
    /// `down`; uploads report no progress yet (`ponytail:` the upload is one request).
    pub direction: &'static str,
    pub state: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub error: Option<String>,
}

/// A file the user picked to send.
#[derive(Debug, Clone)]
pub struct Picked {
    pub name: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

/// The files of the open vault on this device.
pub struct ChatFiles {
    pub db: VaultDb,
    pub store: ChatFileStore,
    pub resolver: Arc<dyn Resolver>,
    pub progress: Option<Arc<dyn Fn(Progress) + Send + Sync>>,
    /// A message with attachments arrived, or the user asked for a download.
    pub wake: Notify,
    /// This device's `vault_device_uuid`, for its download limit.
    pub device: Uuid,
}

impl std::fmt::Debug for ChatFiles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatFiles").finish_non_exhaustive()
    }
}

/// An uploaded file: what the message carries and what this device keeps.
struct Uploaded {
    attachment: FileAttachment,
    signer: Keys,
    plaintext: Vec<u8>,
}

impl ChatFiles {
    fn report(
        &self,
        attachment_id: &str,
        state: &str,
        bytes_done: u64,
        size: u64,
        error: Option<&str>,
    ) {
        if let Some(progress) = &self.progress {
            progress(Progress {
                attachment_id: attachment_id.to_owned(),
                direction: "down",
                state: state.to_owned(),
                bytes_done,
                bytes_total: size,
                error: error.map(str::to_owned),
            });
        }
    }

    /// The vault's Blossom-Server, checked; `chat.file.noServer` when the user chose none (FR-038).
    async fn server(&self) -> Result<Option<Server>> {
        let Some(address) = self.db.read(|r| Ok(settings::blossom_server(r)?)).await? else {
            return Ok(None);
        };
        Server::new(&address, self.resolver.as_ref())
            .await
            .map(Some)
            .map_err(blossom_error)
    }

    /// Sends `text` and `files` to the direct conversation `thread_id`: uploads every file first,
    /// keeps the own copies, then queues the message (FR-031). A retry with the same `event_id`
    /// answers the first message and uploads nothing.
    pub async fn send(
        &self,
        me: &Me,
        thread_id: Uuid,
        event_id: &str,
        text: &str,
        files: Vec<Picked>,
    ) -> Result<Uuid> {
        let (check_me, check_event) = (me.clone(), event_id.to_owned());
        let (sent, peer) = self
            .db
            .read(move |r| {
                Ok((
                    send::sent_already(r, &check_me, thread_id, &check_event)?,
                    send::direct_peer(r, thread_id)?,
                ))
            })
            .await?;
        if let Some(message_id) = sent {
            return Ok(message_id);
        }
        let Some(peer) = peer else {
            return Err(invalid("chat.file.notInNotes"));
        };
        crate::chat::peer::envelope::checked_body(text, files.len())?;
        let server = self
            .server()
            .await?
            .ok_or_else(|| invalid("chat.file.noServer"))?;
        let message_id = ids::message_id(thread_id, &me.vault_hex, event_id);
        let mut uploaded = Vec::with_capacity(files.len());
        for file in files {
            uploaded.push(self.upload(&server, file).await?);
        }
        for (index, file) in uploaded.iter().enumerate() {
            let id = ids::attachment_id(message_id, index).to_string();
            self.store.put(&id, &file.plaintext).await?;
        }
        let pending = self.expected_devices(me, &peer).await?;
        let (me, event_id, text) = (me.clone(), event_id.to_owned(), text.to_owned());
        let base = server_address(&server);
        let now = now_ms();
        self.db
            .write(move |tx| {
                let attachments = uploaded
                    .iter()
                    .map(|file| Attachment::File(file.attachment.clone()))
                    .collect();
                let id = send::message(tx, &me, thread_id, &event_id, &text, attachments, now)?;
                for (index, file) in uploaded.iter().enumerate() {
                    let attachment_id = ids::attachment_id(id, index).to_string();
                    tx.execute(
                        "INSERT OR REPLACE INTO chat_blobs_no_sync (blob_sha256, server, signer, \
                           upload_secret, attachment_id, pending_devices, uploaded_at) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![
                            file.attachment.blossom.sha256,
                            base,
                            file.signer.public_key().to_hex(),
                            file.signer.secret_key().to_secret_bytes().to_vec(),
                            attachment_id,
                            pending,
                            now
                        ],
                    )?;
                    set_state(
                        tx,
                        &attachment_id,
                        "stored",
                        file.attachment.size,
                        None,
                        now,
                    )?;
                }
                Ok(id)
            })
            .await
    }

    /// Encrypts and uploads one file with a fresh key (research R12), asking the server first.
    async fn upload(&self, server: &Server, file: Picked) -> Result<Uploaded> {
        if file.bytes.len() as u64 > crypto::MAX_PLAINTEXT {
            return Err(invalid("chat.file.tooLarge"));
        }
        let key = FileKey::generate()?;
        let sealing = key.clone();
        let (plaintext, sealed) = tokio::task::spawn_blocking(move || {
            let sealed = crypto::encrypt(&sealing, &file.bytes);
            (file.bytes, sealed)
        })
        .await
        .map_err(|e| invalid(&e.to_string()))?;
        let sealed = sealed?;
        let sha256 = crypto::sha256_hex(&sealed);
        let signer = Keys::generate();
        let resolver = self.resolver.as_ref();
        server
            .check_upload(&signer, &sha256, sealed.len() as u64, resolver)
            .await
            .map_err(blossom_error)?;
        let descriptor = server
            .upload(&signer, sealed, resolver)
            .await
            .map_err(blossom_error)?;
        Ok(Uploaded {
            attachment: FileAttachment {
                name: file.name,
                mime: file.mime,
                size: plaintext.len() as u64,
                sha256: crypto::sha256_hex(&plaintext),
                blossom: BlobRef {
                    url: descriptor.url,
                    sha256,
                    key: hex(&key.key),
                },
            },
            signer,
            plaintext,
        })
    }

    /// Every device that will download the file: the recipient's and this vault's other ones
    /// (research R13), as a JSON list of device keys.
    async fn expected_devices(&self, me: &Me, peer: &str) -> Result<String> {
        let (me, peer) = (me.clone(), peer.to_owned());
        let devices = self
            .db
            .read(move |r| {
                let mut devices = targets::of(r, &me, &peer)?.devices;
                devices.extend(targets::of(r, &me, &me.vault_hex)?.devices);
                Ok(devices)
            })
            .await?;
        let devices: Vec<String> = devices.iter().map(|device| hex(device)).collect();
        serde_json::to_string(&devices).map_err(|e| invalid(&e.to_string()))
    }

    /// Downloads, checks, decrypts and keeps the file of attachment `id` (FR-032).
    pub async fn download(&self, id: &str) -> Result<()> {
        let attachment_id = id.to_owned();
        let row = self
            .db
            .read(move |r| {
                r.query_row(
                    "SELECT blossom_url, blob_sha256, blob_key, sha256, COALESCE(size, 0) \
                     FROM chat_attachments WHERE id = ?1 AND kind = 'file'",
                    params![attachment_id],
                    |row| {
                        Ok((
                            row.get::<_, Option<String>>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<Vec<u8>>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, i64>(4)?,
                        ))
                    },
                )
            })
            .await?;
        let Some((Some(url), Some(blob), Some(key), Some(sha256), size)) = row else {
            return Err(HolziError::NotFound {
                name: "chat attachment".into(),
            });
        };
        let key: [u8; 32] = key.try_into().map_err(|_| invalid("chat.file.damaged"))?;
        let size = u64::try_from(size).unwrap_or(0);
        if size > crypto::MAX_PLAINTEXT {
            self.record(id, "failed", 0, size, Some("chat.file.tooLarge"))
                .await?;
            return Err(invalid("chat.file.tooLarge"));
        }
        self.record(id, "downloading", 0, size, None).await?;
        let own = self.server().await.ok().flatten();
        let mut last = tokio::time::Instant::now();
        let mut on_progress = |done: u64| {
            if last.elapsed() >= PROGRESS_EVERY {
                last = tokio::time::Instant::now();
                self.report(id, "downloading", done, size, None);
            }
        };
        let fetched = blossom::download(
            &url,
            &blob,
            crypto::ciphertext_len(size),
            own.as_ref(),
            self.resolver.as_ref(),
            &mut on_progress,
        )
        .await;
        let sealed = match fetched {
            Ok(sealed) => sealed,
            Err(BlossomError::Gone) => return self.record(id, "gone", 0, size, None).await,
            Err(error) => {
                let code = error.code();
                self.record(id, "failed", 0, size, Some(&code)).await?;
                return Err(blossom_error(error));
            }
        };
        let opened = tokio::task::spawn_blocking(move || crypto::decrypt(&key, &sealed, &sha256))
            .await
            .map_err(|e| invalid(&e.to_string()))?;
        let plaintext = match opened {
            Ok(plaintext) => plaintext,
            Err(error) => {
                self.record(id, "failed", 0, size, Some("chat.file.damaged"))
                    .await?;
                return Err(error);
            }
        };
        if let Err(error) = self.store.put(id, &plaintext).await {
            self.record(id, "failed", 0, size, Some("chat.file.storeFailed"))
                .await?;
            return Err(error);
        }
        self.record(id, "stored", size, size, None).await
    }

    async fn record(
        &self,
        id: &str,
        state: &'static str,
        done: u64,
        size: u64,
        error: Option<&str>,
    ) -> Result<()> {
        let (attachment_id, kept) = (id.to_owned(), error.map(str::to_owned));
        let now = now_ms();
        self.db
            .write(move |tx| {
                set_state(tx, &attachment_id, state, done, kept.as_deref(), now)?;
                Ok(())
            })
            .await?;
        // After the row, so a window that reloads on the event reads the new state.
        self.report(id, state, done, size, error);
        Ok(())
    }

    /// Downloads every file this device has not looked at yet, up to its limit; marks larger ones
    /// `pending`. `restart` (at the vault's opening) also takes the ones cut off or failed before.
    pub async fn download_due(&self, restart: bool) -> Result<()> {
        let device = self.device;
        let (limit, due) = self
            .db
            .read(move |r| {
                let limit = settings::auto_limit(r, device)?;
                let states = if restart {
                    "f.state IS NULL OR f.state IN ('downloading', 'failed')"
                } else {
                    "f.state IS NULL"
                };
                let due = r.query_map(
                    &format!(
                        "SELECT a.id, COALESCE(a.size, 0) FROM chat_attachments a \
                         LEFT JOIN chat_attachment_files_no_sync f ON f.attachment_id = a.id \
                         WHERE a.kind = 'file' AND ({states})"
                    ),
                    params![],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
                )?;
                Ok((limit, due))
            })
            .await?;
        for (id, size) in due {
            let size = u64::try_from(size).unwrap_or(0);
            if size > limit {
                self.record(&id, "pending", 0, size, None).await?;
                continue;
            }
            if let Err(error) = self.download(&id).await {
                log::info!("chat: a file was not downloaded: {error}");
            }
        }
        Ok(())
    }

    /// Downloads until the vault closes: what is due at the start, after each wake and every
    /// [`LOOK_AGAIN`].
    pub async fn run(self: Arc<Self>, closing: CancellationToken) {
        let mut tick =
            tokio::time::interval_at(tokio::time::Instant::now() + LOOK_AGAIN, LOOK_AGAIN);
        let mut restart = true;
        loop {
            if let Err(error) = self.download_due(restart).await {
                log::warn!("chat: due downloads not looked at: {error}");
            }
            restart = false;
            tokio::select! {
                () = closing.cancelled() => return,
                () = self.wake.notified() => {}
                _ = tick.tick() => {}
            }
        }
    }
}

/// Where a file stands on this device.
fn set_state(
    tx: &mut haex_crdt::CrdtTransaction<'_>,
    attachment_id: &str,
    state: &str,
    bytes_done: u64,
    error: Option<&str>,
    now: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO chat_attachment_files_no_sync (attachment_id, state, bytes_done, error, \
           updated_at) VALUES (?1, ?2, ?3, ?4, ?5) \
         ON CONFLICT (attachment_id) DO UPDATE SET state = excluded.state, \
           bytes_done = excluded.bytes_done, error = excluded.error, updated_at = excluded.updated_at",
        params![
            attachment_id,
            state,
            i64::try_from(bytes_done).unwrap_or(i64::MAX),
            error,
            now
        ],
    )?;
    Ok(())
}

/// The server's address as the blob rows keep it, to delete there later.
fn server_address(server: &Server) -> String {
    server.address().to_owned()
}

fn blossom_error(error: BlossomError) -> HolziError {
    let reason = match &error {
        BlossomError::Refused {
            reason: Some(reason),
            ..
        } => format!("{}|{reason}", error.code()),
        _ => error.code(),
    };
    HolziError::InvalidInput { reason }
}

fn invalid(reason: &str) -> HolziError {
    HolziError::InvalidInput {
        reason: reason.to_owned(),
    }
}

#[cfg(test)]
#[path = "transfers_tests.rs"]
mod tests;
