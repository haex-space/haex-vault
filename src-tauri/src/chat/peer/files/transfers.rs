//! Moving chat files (spec 072 US3, FR-031 to FR-037, research R11–R14). Sending: every file is
//! encrypted and uploaded to the vault's Blossom-Server before the message is queued, and this
//! device keeps its own copy. Receiving: every device downloads a file itself, by itself up to its
//! limit (FR-036), on request above it. A download that was cut off (vault closed, app ended) starts
//! again when the vault opens next (FR-037). Where a file stands on this device is device-local
//! (`chat_attachment_files_no_sync`).

use std::io::{self, Cursor, Read};
use std::sync::Arc;
use std::time::Duration;

use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};
use haex_crdt::rusqlite::params;
use nostr::key::Keys;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::blossom::{self, BlossomError, Server, UploadBody};
use super::crypto::{self, Decryptor, FileKey, SealedFacts};
use super::store::ChatFileStore;
use crate::chat::peer::delivery::now_ms;
use crate::chat::peer::envelope::{self, Attachment, BlobRef, FileAttachment, INLINE_MIME};
use crate::chat::peer::session::Me;
use crate::chat::peer::{ids, send, settings, targets};
use crate::error::{HaexError, Result};
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
    /// `down`; an upload reports no progress yet: the message does not exist for the window until
    /// it is queued, so there is nothing to show it on (`ponytail:`).
    pub direction: &'static str,
    pub state: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub error: Option<String>,
}

/// Where the bytes of a file to send come from. Sending reads them more than once (the hash is
/// needed before the upload), so every pass opens the source again; nothing holds the file.
#[derive(Clone)]
pub struct Source(Arc<dyn Fn() -> Result<Box<dyn Read + Send>> + Send + Sync>);

impl Source {
    /// Bytes that are in memory, such as a long text.
    pub fn bytes(bytes: Vec<u8>) -> Self {
        let bytes: Arc<[u8]> = bytes.into();
        Self(Arc::new(move || {
            Ok(Box::new(Cursor::new(Arc::clone(&bytes))) as Box<dyn Read + Send>)
        }))
    }

    /// A file that `open` opens for reading, once for every pass.
    pub fn opened_by(
        open: impl Fn() -> Result<Box<dyn Read + Send>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(open))
    }

    fn open(&self) -> Result<Box<dyn Read + Send>> {
        (self.0)()
    }
}

impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Source(..)")
    }
}

/// A file the user picked to send.
#[derive(Debug, Clone)]
pub struct Picked {
    pub name: String,
    pub mime: String,
    pub source: Source,
    /// The file is the full text of the message (research R2), not an attachment of the user.
    pub inline: bool,
}

impl Picked {
    /// A file whose bytes are in memory.
    pub fn in_memory(name: &str, mime: &str, bytes: Vec<u8>, inline: bool) -> Self {
        Self {
            name: name.to_owned(),
            mime: mime.to_owned(),
            source: Source::bytes(bytes),
            inline,
        }
    }
}

/// The bound of the files of one message together (FR-034).
pub const MAX_MESSAGE_PLAINTEXT: u64 = crypto::MAX_PLAINTEXT;

/// What a message may still take after `total` bytes of files.
pub fn remaining_message_limit(total: u64) -> Result<u64> {
    MAX_MESSAGE_PLAINTEXT
        .checked_sub(total)
        .ok_or_else(|| invalid("chat.file.tooLarge"))
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

/// A file that was read once to learn its size and hashes; it still has to be uploaded.
struct Prepared {
    file: Picked,
    key: FileKey,
    facts: SealedFacts,
}

/// An uploaded file: what the message carries and what this device keeps.
struct Uploaded {
    attachment: FileAttachment,
    signer: Keys,
    source: Source,
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
        // Every file is read once first: the server is asked, and the upload signed, with hashes of
        // the whole file, and the files of one message may together be no more than the limit.
        let mut prepared = Vec::with_capacity(files.len());
        let mut total = 0_u64;
        for file in files {
            let key = FileKey::generate()?;
            let facts = self.measure(&file, &key).await?;
            total += facts.plaintext_len;
            remaining_message_limit(total)?;
            prepared.push(Prepared { file, key, facts });
        }
        let mut uploaded = Vec::with_capacity(prepared.len());
        for file in prepared {
            uploaded.push(self.upload(&server, file).await?);
        }
        for (index, file) in uploaded.iter().enumerate() {
            let id = ids::attachment_id(message_id, index).to_string();
            self.keep(&id, &file.source, &file.attachment.sha256)
                .await?;
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

    /// Sends a text that does not fit one wrap (research R2): the whole text as the file
    /// `message.txt`, marked `inline`, and its preview as the text of the message. `text` is the
    /// checked, trimmed text; the rest is as for [`Self::send`].
    pub async fn send_long_text(
        &self,
        me: &Me,
        thread_id: Uuid,
        event_id: &str,
        text: &str,
    ) -> Result<Uuid> {
        let file = Picked::in_memory("message.txt", INLINE_MIME, text.as_bytes().to_vec(), true);
        self.send(
            me,
            thread_id,
            event_id,
            &envelope::preview(text),
            vec![file],
        )
        .await
    }

    /// Reads `file` once, sealing it as it will be sealed for the upload, and answers its length and
    /// hashes. Nothing is kept (research R14 amendment).
    async fn measure(&self, file: &Picked, key: &FileKey) -> Result<SealedFacts> {
        let (source, key) = (file.source.clone(), key.clone());
        tokio::task::spawn_blocking(move || crypto::seal_stream(&key, source.open()?, |_| Ok(())))
            .await
            .map_err(|e| invalid(&e.to_string()))?
    }

    /// Encrypts and uploads one file with its key (research R12), asking the server first. The file
    /// is sealed a second time while it is sent, a segment at a time, and must come out as it did
    /// the first time; a file that changed in between is not sent.
    async fn upload(&self, server: &Server, prepared: Prepared) -> Result<Uploaded> {
        let Prepared { file, key, facts } = prepared;
        let signer = Keys::generate();
        let resolver = self.resolver.as_ref();
        server
            .check_upload(
                &signer,
                &facts.ciphertext_sha256,
                facts.ciphertext_len,
                resolver,
            )
            .await
            .map_err(blossom_error)?;
        let (sender, receiver) = mpsc::channel::<io::Result<Vec<u8>>>(2);
        let body = UploadBody {
            sha256: facts.ciphertext_sha256.clone(),
            size: facts.ciphertext_len,
            chunks: Box::pin(receiver),
        };
        let sealing = {
            let (source, key, expected) = (file.source.clone(), key.clone(), facts.clone());
            tokio::task::spawn_blocking(move || seal_into(&source, &key, &expected, sender))
        };
        let mut progress = |_: u64| {};
        let (sent, sealed) = tokio::join!(
            server.upload(&signer, body, resolver, &mut progress),
            sealing
        );
        let sealed = sealed.map_err(|e| invalid(&e.to_string()))?;
        let descriptor = match (sent, sealed) {
            (Ok(descriptor), Ok(())) => descriptor,
            (Ok(descriptor), Err(error)) => {
                // The server took a blob that is not the file it was told about: take it down.
                let _ = server.delete(&signer, &descriptor.sha256, resolver).await;
                return Err(error);
            }
            // Reading failed or the file changed, and the upload broke because of it.
            (Err(_), Err(error)) if !is_aborted(&error) => return Err(error),
            (Err(error), _) => return Err(blossom_error(error)),
        };
        Ok(Uploaded {
            attachment: FileAttachment {
                name: file.name,
                mime: file.mime,
                size: facts.plaintext_len,
                sha256: facts.plaintext_sha256,
                blossom: BlobRef {
                    url: descriptor.url,
                    sha256: facts.ciphertext_sha256,
                    key: hex(&key.key),
                },
                inline: file.inline,
            },
            signer,
            source: file.source,
        })
    }

    /// Keeps this device's own copy of a sent file as attachment `id`: the source is read a third
    /// time into the store, and the copy shows only if it is the file that was sent.
    async fn keep(&self, id: &str, source: &Source, plaintext_sha256: &str) -> Result<()> {
        let mut writer = self.store.writer(id).await?;
        let (mut pieces, reading) = read_in_pieces(source.clone());
        let mut failed = None;
        while let Some(piece) = pieces.next().await {
            let written = match piece {
                Ok(piece) => writer.push(&piece).await,
                Err(error) => Err(invalid(&error.to_string())),
            };
            if let Err(error) = written {
                failed = Some(error);
                break;
            }
        }
        // Without the pieces the reader stops at its next send.
        drop(pieces);
        let read = reading.await.map_err(|e| invalid(&e.to_string()))?;
        match (failed, read) {
            (None, Ok(sha256)) if sha256.eq_ignore_ascii_case(plaintext_sha256) => {
                writer.commit().await
            }
            (None, Ok(_)) => {
                writer.abort().await;
                Err(invalid("chat.file.changed"))
            }
            (failed, read) => {
                writer.abort().await;
                Err(failed
                    .or(read.err())
                    .unwrap_or_else(|| invalid("chat.file.changed")))
            }
        }
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

    /// The full text of the `inline` attachment `id` from this device's copy (research R2); not a
    /// reader of other files. `NotFound` while this device holds no copy yet.
    pub async fn text(&self, id: &str) -> Result<String> {
        let attachment_id = id.to_owned();
        let inline = self
            .db
            .read(move |r| {
                r.query_row(
                    "SELECT inline FROM chat_attachments WHERE id = ?1 AND kind = 'file'",
                    params![attachment_id],
                    |row| row.get::<_, i64>(0),
                )
            })
            .await?;
        if inline != Some(1) {
            return Err(HaexError::NotFound {
                name: "chat text".into(),
            });
        }
        let bytes = self
            .store
            .get(id)
            .await?
            .ok_or_else(|| HaexError::NotFound {
                name: "chat text file".into(),
            })?;
        String::from_utf8(bytes).map_err(|_| invalid("chat.file.damaged"))
    }

    /// Downloads, checks, decrypts and keeps the file of attachment `id` (FR-032). The bytes go into
    /// the store as they come; the file shows only after both hashes were checked.
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
            return Err(HaexError::NotFound {
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
        let received = self
            .receive(id, &url, &blob, &key, &sha256, size, own.as_ref())
            .await;
        match received {
            Ok(()) => self.record(id, "stored", size, size, None).await,
            Err(Receive::Gone) => self.record(id, "gone", 0, size, None).await,
            Err(Receive::Blossom(error)) => {
                let code = error.code();
                self.record(id, "failed", 0, size, Some(&code)).await?;
                Err(blossom_error(error))
            }
            Err(Receive::Damaged(error)) => {
                self.record(id, "failed", 0, size, Some("chat.file.damaged"))
                    .await?;
                Err(error)
            }
            Err(Receive::Store(error)) => {
                self.record(id, "failed", 0, size, Some("chat.file.storeFailed"))
                    .await?;
                Err(error)
            }
        }
    }

    /// The streaming part of [`Self::download`]: bytes in, plaintext out into the store.
    #[allow(clippy::too_many_arguments)]
    async fn receive(
        &self,
        id: &str,
        url: &str,
        blob: &str,
        key: &[u8; 32],
        plaintext_sha256: &str,
        size: u64,
        own: Option<&Server>,
    ) -> std::result::Result<(), Receive> {
        let mut stream = blossom::download(
            url,
            blob,
            crypto::ciphertext_len(size),
            own,
            self.resolver.as_ref(),
        )
        .await
        .map_err(Receive::from)?;
        let mut writer = self.store.writer(id).await.map_err(Receive::Store)?;
        let mut decryptor = Decryptor::new(key, plaintext_sha256);
        let mut last = tokio::time::Instant::now();
        let outcome: std::result::Result<(), Receive> = async {
            loop {
                let Some(piece) = stream.chunk().await.map_err(Receive::from)? else {
                    break;
                };
                let plain = decryptor.push(&piece).map_err(Receive::Damaged)?;
                if !plain.is_empty() {
                    writer.push(&plain).await.map_err(Receive::Store)?;
                }
                if last.elapsed() >= PROGRESS_EVERY {
                    last = tokio::time::Instant::now();
                    self.report(id, "downloading", stream.received(), size, None);
                }
            }
            // The ciphertext is the blob that was named; now the plaintext is checked.
            let rest = decryptor.finish().map_err(Receive::Damaged)?;
            writer.push(&rest).await.map_err(Receive::Store)
        }
        .await;
        match outcome {
            Ok(()) => writer.commit().await.map_err(Receive::Store),
            Err(error) => {
                writer.abort().await;
                Err(error)
            }
        }
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

/// Why a download did not end in a stored file.
enum Receive {
    /// The blob is no longer on the server.
    Gone,
    Blossom(BlossomError),
    /// A hash or a segment did not check out.
    Damaged(HaexError),
    /// This device could not keep the file.
    Store(HaexError),
}

impl From<BlossomError> for Receive {
    fn from(error: BlossomError) -> Self {
        match error {
            BlossomError::Gone => Self::Gone,
            // The blob is not the one the message named: damaged, not a network matter.
            BlossomError::Mismatch => Self::Damaged(invalid("chat.file.damaged")),
            other => Self::Blossom(other),
        }
    }
}

/// Seals `source` into `sender` piece by piece and checks that it comes out as `expected` did. A
/// failure is also put into the stream, so that the upload breaks instead of ending as if it were
/// complete.
fn seal_into(
    source: &Source,
    key: &FileKey,
    expected: &SealedFacts,
    mut sender: mpsc::Sender<io::Result<Vec<u8>>>,
) -> Result<()> {
    let outcome = source.open().and_then(|reader| {
        crypto::seal_stream(key, reader, |piece| {
            futures::executor::block_on(sender.send(Ok(piece.to_vec()))).map_err(|_| aborted())
        })
    });
    let failure = match outcome {
        Ok(again) if &again == expected => return Ok(()),
        Ok(_) => invalid("chat.file.changed"),
        Err(error) => error,
    };
    if !is_aborted(&failure) {
        let _ =
            futures::executor::block_on(sender.send(Err(io::Error::other(failure.to_string()))));
    }
    Err(failure)
}

/// Reads `source` on a blocking thread and answers its bytes in pieces; the handle answers the
/// SHA-256 of what was read. A reader whose pieces are not taken any more stops.
fn read_in_pieces(
    source: Source,
) -> (
    mpsc::Receiver<io::Result<Vec<u8>>>,
    JoinHandle<Result<String>>,
) {
    let (mut sender, receiver) = mpsc::channel::<io::Result<Vec<u8>>>(2);
    let reading = tokio::task::spawn_blocking(move || {
        let mut reader = source.open()?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0; crypto::SEGMENT];
        loop {
            let n = match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    let _ = futures::executor::block_on(
                        sender.send(Err(io::Error::new(error.kind(), error.to_string()))),
                    );
                    return Err(invalid("chat.file.unreadable"));
                }
            };
            hasher.update(&buf[..n]);
            futures::executor::block_on(sender.send(Ok(buf[..n].to_vec())))
                .map_err(|_| aborted())?;
        }
        Ok(hex(&hasher.finalize()))
    });
    (receiver, reading)
}

/// The consumer of a stream is gone, so reading it further is pointless.
fn aborted() -> HaexError {
    invalid("chat.file.aborted")
}

fn is_aborted(error: &HaexError) -> bool {
    matches!(error, HaexError::InvalidInput { reason } if reason == "chat.file.aborted")
}

/// The server's address as the blob rows keep it, to delete there later.
fn server_address(server: &Server) -> String {
    server.address().to_owned()
}

fn blossom_error(error: BlossomError) -> HaexError {
    let reason = match &error {
        BlossomError::Refused {
            reason: Some(reason),
            ..
        } => format!("{}|{reason}", error.code()),
        _ => error.code(),
    };
    HaexError::InvalidInput { reason }
}

fn invalid(reason: &str) -> HaexError {
    HaexError::InvalidInput {
        reason: reason.to_owned(),
    }
}

#[cfg(test)]
#[path = "transfers_tests.rs"]
mod tests;
