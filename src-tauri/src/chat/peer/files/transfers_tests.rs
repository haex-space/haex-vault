//! Files between two vaults over a Nostr-Relay and a Blossom-Server in the process (spec 072 T059,
//! US3 scenarios 1, 2, 5, 6, 8): the receiver open or closed, above the download limit, refused by
//! the server or without one, a blob that is gone, and the receiver's second device downloading
//! itself.

use std::sync::{Arc, Barrier, Mutex};

use super::*;
use crate::chat::peer::files::test_server::BlossomTestServer;
use crate::chat::peer::messages::PeerMessage;
use crate::chat::peer::test_support::{mock_relay, two_vaults, ChatDevice};
use crate::passwords::access::Caller;
use crate::remote_storage::address::SystemResolver;

const PLAN: &[u8] = b"%PDF-1.7 the plan for saturday, never in plaintext on a server";

fn picked(bytes: &[u8]) -> Picked {
    Picked::in_memory("Plan.pdf", "application/pdf", bytes.to_vec(), false)
}

#[test]
fn one_message_cannot_exceed_the_aggregate_file_size_limit() {
    assert_eq!(
        remaining_message_limit(0).expect("empty selection"),
        MAX_MESSAGE_PLAINTEXT
    );
    assert_eq!(
        remaining_message_limit(MAX_MESSAGE_PLAINTEXT - 1).expect("one byte left"),
        1
    );
    assert_eq!(
        remaining_message_limit(MAX_MESSAGE_PLAINTEXT).expect("limit reached"),
        0
    );
    assert!(remaining_message_limit(MAX_MESSAGE_PLAINTEXT + 1).is_err());
}

async fn send_file(from: &ChatDevice, to: &ChatDevice, text: &str, bytes: &[u8]) -> Result<Uuid> {
    send_picked(from, to, text, picked(bytes)).await
}

async fn send_picked(from: &ChatDevice, to: &ChatDevice, text: &str, file: Picked) -> Result<Uuid> {
    let thread = from.thread_with(to);
    let (own, peer) = (from.me.vault_hex.clone(), to.me.vault_hex.clone());
    from.db
        .write_blocking(move |tx| {
            Ok(crate::chat::peer::store::open_direct(
                tx,
                &own,
                &peer,
                now_ms(),
            )?)
        })
        .expect("open");
    let files = from.session.files.get().expect("files").clone();
    let event = crate::chat::peer::envelope::new_event_id();
    let sent = files.send(&from.me, thread, &event, text, vec![file]).await;
    from.session.kick.notify_one();
    sent
}

fn file_state(messages: &[PeerMessage]) -> Option<String> {
    messages
        .iter()
        .flat_map(|m| &m.attachments)
        .next()
        .and_then(|a| a.state.clone())
}

fn attachment_id(messages: &[PeerMessage]) -> String {
    messages
        .iter()
        .flat_map(|m| &m.attachments)
        .next()
        .expect("an attachment")
        .id
        .clone()
}

async fn stored(device: &ChatDevice, id: &str) -> Option<Vec<u8>> {
    device
        .session
        .files
        .get()
        .expect("files")
        .store
        .get(id)
        .await
        .expect("read")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_reaches_an_open_receiver_and_both_keep_a_copy() {
    let (_relay, url) = mock_relay().await;
    let blossom = BlossomTestServer::start().await;
    let (anna, ben) = two_vaults(&[url]).await;
    let reports = Arc::new(Mutex::new(Vec::new()));
    let heard = Arc::clone(&reports);
    anna.enable_files(&blossom.url, None);
    ben.enable_files(
        &blossom.url,
        Some(Arc::new(move |progress: Progress| {
            heard.lock().expect("reports").push(progress)
        })),
    );
    anna.start().await;
    ben.start().await;

    send_file(&anna, &ben, "Hier der Plan", PLAN)
        .await
        .expect("send");
    let at_anna = anna.messages_with(&ben);
    assert_eq!(at_anna[0].text.as_deref(), Some("Hier der Plan"));
    assert_eq!(at_anna[0].attachments[0].name.as_deref(), Some("Plan.pdf"));
    assert_eq!(file_state(&at_anna).as_deref(), Some("stored"));
    let id = attachment_id(&at_anna);
    assert_eq!(stored(&anna, &id).await.as_deref(), Some(PLAN));

    let at_ben = ben
        .wait_for(&anna, "the file at Ben", |m| {
            file_state(m).as_deref() == Some("stored")
        })
        .await;
    assert_eq!(attachment_id(&at_ben), id, "both name the same attachment");
    assert_eq!(at_ben[0].attachments[0].size, PLAN.len() as i64);
    assert_eq!(stored(&ben, &id).await.as_deref(), Some(PLAN));
    let states: Vec<String> = reports
        .lock()
        .expect("reports")
        .iter()
        .map(|p| p.state.clone())
        .collect();
    assert_eq!(
        states.last().map(String::as_str),
        Some("stored"),
        "{states:?}"
    );

    let on_server: Vec<u8> = blossom.log().into_iter().flat_map(|l| l.body).collect();
    assert!(
        !on_server.windows(PLAN.len()).any(|w| w == PLAN),
        "the server only saw ciphertext"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_closed_receiver_gets_the_file_when_it_opens_while_the_sender_is_gone() {
    let (_relay, url) = mock_relay().await;
    let blossom = BlossomTestServer::start().await;
    let (anna, ben) = two_vaults(&[url]).await;
    anna.enable_files(&blossom.url, None);
    ben.enable_files(&blossom.url, None);
    // Ben's device was online once, so its chat key record is on the relay (spec 095).
    ben.start().await;
    ben.stop();
    anna.start().await;
    send_file(&anna, &ben, "", PLAN).await.expect("send");
    anna.wait_for(&ben, "sent", |m| {
        m.first()
            .and_then(|m| m.delivery)
            .is_some_and(|d| d != crate::chat::peer::messages::DeliveryState::Waiting)
    })
    .await;
    anna.stop();

    ben.start().await;
    let at_ben = ben
        .wait_for(&anna, "the file", |m| {
            file_state(m).as_deref() == Some("stored")
        })
        .await;
    assert_eq!(at_ben[0].text.as_deref(), Some(""), "a file without text");
    assert_eq!(
        stored(&ben, &attachment_id(&at_ben)).await.as_deref(),
        Some(PLAN)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_above_the_limit_waits_for_the_user_and_a_gone_one_says_so() {
    let (_relay, url) = mock_relay().await;
    let blossom = BlossomTestServer::start().await;
    let (anna, ben) = two_vaults(&[url]).await;
    anna.enable_files(&blossom.url, None);
    let ben_files = ben.enable_files(&blossom.url, None);
    let device = ben_files.device;
    ben.db
        .write_blocking(move |tx| Ok(settings::set_auto_limit(tx, device, 10)?))
        .expect("limit");
    anna.start().await;
    ben.start().await;

    send_file(&anna, &ben, "groß", PLAN).await.expect("send");
    let at_ben = ben
        .wait_for(&anna, "pending", |m| {
            file_state(m).as_deref() == Some("pending")
        })
        .await;
    let id = attachment_id(&at_ben);
    assert_eq!(stored(&ben, &id).await, None);
    ben_files.download(&id).await.expect("download on request");
    assert_eq!(stored(&ben, &id).await.as_deref(), Some(PLAN));

    // A second file whose blob is deleted before Ben asks for it.
    send_file(&anna, &ben, "weg", b"gone soon, larger than the limit")
        .await
        .expect("send");
    let at_ben = ben
        .wait_for(&anna, "the second pending", |m| {
            m.iter()
                .filter(|m| {
                    m.attachments.first().and_then(|a| a.state.as_deref()) == Some("pending")
                })
                .count()
                == 1
        })
        .await;
    let second = at_ben
        .iter()
        .find(|m| m.text.as_deref() == Some("weg"))
        .expect("second");
    let blob = anna
        .db
        .read_blocking(|r| {
            r.query_row(
                "SELECT blob_sha256, upload_secret FROM chat_blobs_no_sync WHERE uploaded_at = \
                   (SELECT MAX(uploaded_at) FROM chat_blobs_no_sync)",
                params![],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
        })
        .expect("blob row")
        .expect("a blob");
    let signer = Keys::new(nostr::key::SecretKey::from_slice(&blob.1).expect("secret"));
    Server::new(
        &blossom.url,
        &crate::remote_storage::address::SystemResolver,
    )
    .await
    .expect("server")
    .delete(
        &signer,
        &blob.0,
        &crate::remote_storage::address::SystemResolver,
    )
    .await
    .expect("delete with the kept upload key");
    ben_files
        .download(&second.attachments[0].id)
        .await
        .expect("gone is no error");
    let at_ben = ben.messages_with(&anna);
    let second = at_ben
        .iter()
        .find(|m| m.text.as_deref() == Some("weg"))
        .expect("second");
    assert_eq!(second.attachments[0].state.as_deref(), Some("gone"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_oversized_attachment_is_rejected_before_downloading() {
    let (_relay, url) = mock_relay().await;
    let blossom = BlossomTestServer::start().await;
    let (anna, ben) = two_vaults(&[url]).await;
    anna.enable_files(&blossom.url, None);
    let ben_files = ben.enable_files(&blossom.url, None);
    let device = ben_files.device;
    ben.db
        .write_blocking(move |tx| Ok(settings::set_auto_limit(tx, device, 0)?))
        .expect("limit");
    anna.start().await;
    ben.start().await;

    send_file(&anna, &ben, "too large", PLAN)
        .await
        .expect("send");
    let messages = ben
        .wait_for(&anna, "pending attachment", |messages| {
            file_state(messages).as_deref() == Some("pending")
        })
        .await;
    let id = attachment_id(&messages);
    let id_for_update = id.clone();
    ben.db
        .write_blocking(move |tx| {
            tx.execute(
                "UPDATE chat_attachments SET size = ?1 WHERE id = ?2",
                params![(crypto::MAX_PLAINTEXT + 1) as i64, id_for_update],
            )?;
            Ok(())
        })
        .expect("tamper with the remote size");
    let before = blossom.log().len();

    let error = ben_files.download(&id).await.expect_err("over the limit");
    assert!(error.to_string().contains("chat.file.tooLarge"));
    assert_eq!(blossom.log().len(), before, "no download request is sent");
    let failed = ben.messages_with(&anna);
    let attachment = failed
        .iter()
        .flat_map(|message| &message.attachments)
        .find(|attachment| attachment.id == id)
        .expect("attachment remains visible");
    assert_eq!(attachment.state.as_deref(), Some("failed"));
    assert_eq!(attachment.error.as_deref(), Some("chat.file.tooLarge"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_store_failure_marks_the_attachment_failed() {
    use crate::chat::peer::files::store::ChatFileStore;
    use crate::chat::peer::files::transfers::ChatFiles;

    let (_relay, url) = mock_relay().await;
    let blossom = BlossomTestServer::start().await;
    let (anna, ben) = two_vaults(&[url]).await;
    anna.enable_files(&blossom.url, None);
    let ben_files = ben.enable_files(&blossom.url, None);
    let device = ben_files.device;
    ben.db
        .write_blocking(move |tx| Ok(settings::set_auto_limit(tx, device, 0)?))
        .expect("limit");
    anna.start().await;
    ben.start().await;

    send_file(&anna, &ben, "cannot store", PLAN)
        .await
        .expect("send");
    let messages = ben
        .wait_for(&anna, "pending attachment", |messages| {
            file_state(messages).as_deref() == Some("pending")
        })
        .await;
    let id = attachment_id(&messages);

    // ChatFileStore creates this path as a directory; an existing file forces that operation to
    // fail without relying on process-specific permissions.
    let blocked_path = tempfile::NamedTempFile::new().expect("blocking file");
    let db = ben.db.clone();
    let store = ChatFileStore::new(blocked_path.path(), Arc::new(move || Ok(db.clone())))
        .expect("store setup");
    let files = ChatFiles {
        db: ben.db.clone(),
        store,
        resolver: Arc::new(crate::remote_storage::address::SystemResolver),
        progress: None,
        wake: tokio::sync::Notify::new(),
        device: ben_files.device,
    };

    assert!(files.download(&id).await.is_err(), "store write fails");
    let failed = ben.messages_with(&anna);
    let attachment = failed
        .iter()
        .flat_map(|message| &message.attachments)
        .find(|attachment| attachment.id == id)
        .expect("attachment remains visible");
    assert_eq!(attachment.state.as_deref(), Some("failed"));
    assert_eq!(attachment.error.as_deref(), Some("chat.file.storeFailed"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_a_server_or_over_its_limit_nothing_is_sent() {
    let blossom = BlossomTestServer::start().await;
    let (anna, ben) = two_vaults(&[]).await;
    let files = anna.enable_files(&blossom.url, None);
    anna.db
        .write_blocking(|tx| Ok(settings::set_blossom_server(tx, None)?))
        .expect("no server");
    let refused = send_file(&anna, &ben, "x", PLAN)
        .await
        .expect_err("no server");
    assert!(
        refused.to_string().contains("chat.file.noServer"),
        "{refused}"
    );

    let server = blossom.url.clone();
    anna.db
        .write_blocking(move |tx| Ok(settings::set_blossom_server(tx, Some(&server))?))
        .expect("server");
    blossom.limit(10, "free accounts take 10 bytes");
    let refused = send_file(&anna, &ben, "x", PLAN)
        .await
        .expect_err("too large");
    assert!(refused.to_string().contains("blossom:413"), "{refused}");
    assert!(
        refused.to_string().contains("free accounts take 10 bytes"),
        "the server's reason is shown: {refused}"
    );
    assert!(anna.messages_with(&ben).is_empty(), "no message queued");
    drop(files);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_receivers_second_device_downloads_the_file_itself() {
    let (_relay, url) = mock_relay().await;
    let blossom = BlossomTestServer::start().await;
    let (anna, ben) = two_vaults(std::slice::from_ref(&url)).await;
    let ben_phone = ben.link();
    anna.learn(&ben).await;
    anna.enable_files(&blossom.url, None);
    ben.enable_files(&blossom.url, None);
    ben_phone.enable_files(&blossom.url, None);
    anna.start().await;
    ben.start().await;
    ben_phone.start().await;

    send_file(&anna, &ben, "für beide", PLAN)
        .await
        .expect("send");
    for device in [&ben, &ben_phone] {
        let found = device
            .wait_for(&anna, "the file on each of Ben's devices", |m| {
                file_state(m).as_deref() == Some("stored")
            })
            .await;
        assert_eq!(
            stored(device, &attachment_id(&found)).await.as_deref(),
            Some(PLAN)
        );
    }
    let pending = anna
        .db
        .read_blocking(|r| {
            r.query_row(
                "SELECT pending_devices FROM chat_blobs_no_sync",
                params![],
                |row| row.get::<_, String>(0),
            )
        })
        .expect("blob")
        .expect("one blob");
    let devices: Vec<String> = serde_json::from_str(&pending).expect("json");
    assert_eq!(devices.len(), 2, "both of Ben's devices: {devices:?}");
}

/// More than one part of the store (8 MiB) and more than eight segments, deterministic.
fn many_segments() -> Vec<u8> {
    (0..9 * 1024 * 1024 + 17)
        .map(|i| (i * 31 % 251) as u8)
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_of_many_segments_goes_through_the_streams_intact() {
    let (_relay, url) = mock_relay().await;
    let blossom = BlossomTestServer::start().await;
    let (anna, ben) = two_vaults(&[url]).await;
    anna.enable_files(&blossom.url, None);
    ben.enable_files(&blossom.url, None);
    anna.start().await;
    ben.start().await;

    let data = many_segments();
    send_file(&anna, &ben, "viel", &data).await.expect("send");
    let at_anna = anna.messages_with(&ben);
    let id = attachment_id(&at_anna);
    assert!(stored(&anna, &id).await.as_deref() == Some(data.as_slice()));

    let at_ben = ben
        .wait_for(&anna, "the file at Ben", |m| {
            file_state(m).as_deref() == Some("stored")
        })
        .await;
    assert_eq!(at_ben[0].attachments[0].size, data.len() as i64);
    assert!(stored(&ben, &id).await.as_deref() == Some(data.as_slice()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_that_changes_between_the_passes_is_not_sent() {
    let (_relay, url) = mock_relay().await;
    let blossom = BlossomTestServer::start().await;
    let (anna, ben) = two_vaults(&[url]).await;
    anna.enable_files(&blossom.url, None);
    ben.enable_files(&blossom.url, None);
    anna.start().await;
    ben.start().await;

    let opened = Arc::new(Mutex::new(0_u32));
    let counted = Arc::clone(&opened);
    let source = Source::opened_by(move || {
        let mut times = counted.lock().expect("count");
        *times += 1;
        let mut bytes = PLAN.to_vec();
        // The second look at the file finds another byte.
        if *times > 1 {
            bytes[5] ^= 1;
        }
        Ok(Box::new(std::io::Cursor::new(bytes)) as Box<dyn Read + Send>)
    });
    let file = Picked {
        name: "Plan.pdf".into(),
        mime: "application/pdf".into(),
        source,
        inline: false,
    };
    let error = send_picked(&anna, &ben, "", file)
        .await
        .expect_err("a changed file is not sent");
    assert!(error.to_string().contains("chat.file.changed"), "{error}");
    assert!(anna.messages_with(&ben).is_empty(), "no message was queued");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_blob_that_is_not_the_named_one_is_dropped_and_leaves_no_file() {
    let (_relay, url) = mock_relay().await;
    let blossom = BlossomTestServer::start().await;
    let (anna, ben) = two_vaults(&[url]).await;
    anna.enable_files(&blossom.url, None);
    let ben_files = ben.enable_files(&blossom.url, None);
    let device = ben_files.device;
    ben.db
        .write_blocking(move |tx| Ok(settings::set_auto_limit(tx, device, 10)?))
        .expect("limit");
    anna.start().await;
    ben.start().await;

    send_file(&anna, &ben, "kaputt", PLAN).await.expect("send");
    let at_ben = ben
        .wait_for(&anna, "pending", |m| {
            file_state(m).as_deref() == Some("pending")
        })
        .await;
    let id = attachment_id(&at_ben);
    let blob = blossom
        .log()
        .into_iter()
        .find(|logged| logged.method == "PUT")
        .map(|put| hex(&Sha256::digest(&put.body)))
        .expect("the upload");
    blossom.tamper(&blob);

    let error = ben_files.download(&id).await.expect_err("damaged");
    assert!(error.to_string().contains("chat.file.damaged"), "{error}");
    assert_eq!(stored(&ben, &id).await, None, "no file shows");
    let at_ben = ben.messages_with(&anna);
    assert_eq!(file_state(&at_ben).as_deref(), Some("failed"));
}

async fn orphan_rows(files: &ChatFiles) -> i64 {
    files
        .db
        .read(|r| {
            r.query_row(
                "SELECT COUNT(*) FROM chat_blobs_no_sync WHERE attachment_id LIKE 'orphan-cleanup:%'",
                params![],
                |row| row.get::<_, i64>(0),
            )
        })
        .await
        .expect("count")
        .unwrap_or(0)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_that_grows_after_it_was_measured_stops_the_copy_and_takes_the_blob_down() {
    let (_relay, url) = mock_relay().await;
    let blossom = BlossomTestServer::start().await;
    let (anna, ben) = two_vaults(&[url]).await;
    anna.enable_files(&blossom.url, None);
    ben.enable_files(&blossom.url, None);
    anna.start().await;
    ben.start().await;

    let opened = Arc::new(Mutex::new(0_u32));
    let counted = Arc::clone(&opened);
    let source = Source::opened_by(move || {
        let mut times = counted.lock().expect("count");
        *times += 1;
        let mut bytes = PLAN.to_vec();
        // The measuring pass and the upload see the file as it was; the copy finds more.
        if *times > 2 {
            bytes.extend_from_slice(b" and more that was appended meanwhile");
        }
        Ok(Box::new(std::io::Cursor::new(bytes)) as Box<dyn Read + Send>)
    });
    let file = Picked {
        name: "Plan.pdf".into(),
        mime: "application/pdf".into(),
        source,
        inline: false,
    };
    let error = send_picked(&anna, &ben, "", file)
        .await
        .expect_err("a file that grew is not sent");
    assert!(error.to_string().contains("chat.file.changed"), "{error}");
    assert!(anna.messages_with(&ben).is_empty(), "no message was queued");

    let uploaded = blossom
        .log()
        .into_iter()
        .find(|logged| logged.method == "PUT")
        .map(|put| hex(&Sha256::digest(&put.body)))
        .expect("the blob was uploaded first");
    assert!(
        !blossom.has(&uploaded),
        "the blob of a message that was never queued is deleted"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_send_that_fails_after_a_copy_was_kept_deletes_the_copies() {
    let (_relay, url) = mock_relay().await;
    let blossom = BlossomTestServer::start().await;
    let (anna, ben) = two_vaults(&[url]).await;
    anna.enable_files(&blossom.url, None);
    ben.enable_files(&blossom.url, None);
    anna.start().await;
    ben.start().await;

    let opened = Arc::new(Mutex::new(0_u32));
    let counted = Arc::clone(&opened);
    // The second file grows between the upload and its own copy, after the first copy was kept.
    let growing = Source::opened_by(move || {
        let mut times = counted.lock().expect("count");
        *times += 1;
        let mut bytes = b"second file".to_vec();
        if *times > 2 {
            bytes.extend_from_slice(b" and more");
        }
        Ok(Box::new(std::io::Cursor::new(bytes)) as Box<dyn Read + Send>)
    });
    let files = vec![
        picked(PLAN),
        Picked {
            name: "Two.txt".into(),
            mime: "text/plain".into(),
            source: growing,
            inline: false,
        },
    ];
    let thread = anna.thread_with(&ben);
    let (own, peer) = (anna.me.vault_hex.clone(), ben.me.vault_hex.clone());
    anna.db
        .write_blocking(move |tx| {
            Ok(crate::chat::peer::store::open_direct(
                tx,
                &own,
                &peer,
                now_ms(),
            )?)
        })
        .expect("open");
    let chat_files = anna.session.files.get().expect("files").clone();
    let event = crate::chat::peer::envelope::new_event_id();
    let error = chat_files
        .send(&anna.me, thread, &event, "", files)
        .await
        .expect_err("the grown file is not sent");
    assert!(error.to_string().contains("chat.file.changed"), "{error}");
    assert!(anna.messages_with(&ben).is_empty(), "no message was queued");

    let message = ids::message_id(thread, &anna.me.vault_hex, &event);
    for index in 0..2 {
        let id = ids::attachment_id(message, index).to_string();
        assert_eq!(
            stored(&anna, &id).await,
            None,
            "file {index} left a copy nothing refers to"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_sends_with_one_event_id_only_upload_once() {
    let (_relay, url) = mock_relay().await;
    let blossom = BlossomTestServer::start().await;
    let (anna, ben) = two_vaults(&[url]).await;
    anna.enable_files(&blossom.url, None);
    ben.enable_files(&blossom.url, None);
    anna.start().await;
    ben.start().await;

    let thread = anna.thread_with(&ben);
    let (own, peer) = (anna.me.vault_hex.clone(), ben.me.vault_hex.clone());
    anna.db
        .write_blocking(move |tx| {
            Ok(crate::chat::peer::store::open_direct(
                tx,
                &own,
                &peer,
                now_ms(),
            )?)
        })
        .expect("open");

    let barrier = Arc::new(Barrier::new(2));
    let source = |bytes: &'static [u8]| {
        let barrier = Arc::clone(&barrier);
        let first_open = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let first = Arc::clone(&first_open);
        Source::opened_by(move || {
            if first.swap(false, std::sync::atomic::Ordering::SeqCst) {
                barrier.wait();
            }
            Ok(Box::new(std::io::Cursor::new(bytes)) as Box<dyn Read + Send>)
        })
    };
    let (files, me, event) = (
        anna.session.files.get().expect("files").clone(),
        anna.me.clone(),
        crate::chat::peer::envelope::new_event_id(),
    );
    let first = {
        let (files, me, event) = (Arc::clone(&files), me.clone(), event.clone());
        let file = Picked {
            name: "First.txt".into(),
            mime: "text/plain".into(),
            source: source(b"first"),
            inline: false,
        };
        tokio::spawn(async move { files.send(&me, thread, &event, "", vec![file]).await })
    };
    let second = {
        let file = Picked {
            name: "Second.txt".into(),
            mime: "text/plain".into(),
            source: source(b"second"),
            inline: false,
        };
        tokio::spawn(async move { files.send(&me, thread, &event, "", vec![file]).await })
    };

    let first_id = first.await.expect("first task").expect("first send");
    let second_id = second.await.expect("second task").expect("second send");
    assert_eq!(
        first_id, second_id,
        "an idempotent retry returns the queued id"
    );
    assert_eq!(
        blossom
            .log()
            .iter()
            .filter(|entry| entry.method == "PUT")
            .count(),
        1,
        "the concurrent retry must not upload a second attachment"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_deletion_that_fails_is_remembered_and_done_when_the_server_is_back() {
    let blossom = BlossomTestServer::start().await;
    let (anna, _ben) = two_vaults(&[]).await;
    let files = anna.enable_files(&blossom.url, None);
    let keys = Keys::generate();
    let live = Server::new(&blossom.url, &SystemResolver)
        .await
        .expect("server");
    let blob = b"ciphertext of a message that was never queued".to_vec();
    let descriptor = live
        .upload(&keys, UploadBody::whole(blob), &SystemResolver, &mut |_| {})
        .await
        .expect("upload");
    assert!(blossom.has(&descriptor.sha256));

    // Nothing listens at the other address: the deletion fails and is kept.
    let gone = Server::new("http://127.0.0.1:1/", &SystemResolver)
        .await
        .expect("server");
    files
        .delete_unqueued(&gone, &[(keys.clone(), descriptor.sha256.clone(), None)])
        .await;
    assert_eq!(orphan_rows(&files).await, 1);
    assert!(blossom.has(&descriptor.sha256), "the blob is still there");

    // The same row for the server that is back.
    let (hash, address, signer, secret) = (
        descriptor.sha256.clone(),
        blossom.url.clone(),
        keys.public_key().to_hex(),
        keys.secret_key().to_secret_bytes().to_vec(),
    );
    files
        .db
        .write(move |tx| {
            tx.execute(
                "UPDATE chat_blobs_no_sync SET server = ?2, signer = ?3, upload_secret = ?4 \
                 WHERE blob_sha256 = ?1",
                params![hash, address, signer, secret],
            )?;
            Ok(())
        })
        .await
        .expect("row");
    files.retry_orphan_deletes().await.expect("retry");
    assert!(!blossom.has(&descriptor.sha256), "the retry deleted it");
    assert_eq!(orphan_rows(&files).await, 0, "and forgot it");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn uploads_are_signed_with_the_upload_key_and_a_missing_key_stops_the_send() {
    let (_relay, url) = mock_relay().await;
    let blossom = BlossomTestServer::start().await;
    let (anna, ben) = two_vaults(&[url]).await;
    anna.enable_files(&blossom.url, None);
    ben.enable_files(&blossom.url, None);
    anna.start().await;
    ben.start().await;
    let passwords = PasswordsService::new(anna.db.clone());
    let entry = upload_key::create(&passwords, &anna.db)
        .await
        .expect("an upload key");

    send_file(&anna, &ben, "Hier der Plan", PLAN)
        .await
        .expect("send");
    let (blob, signer, secret, key_entry) = anna
        .db
        .read_blocking(|r| {
            r.query_row(
                "SELECT blob_sha256, signer, upload_secret, key_entry_id FROM chat_blobs_no_sync",
                params![],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<Vec<u8>>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )
        })
        .expect("blob row")
        .expect("a blob");
    assert_eq!(
        secret, None,
        "the key's only copy is in the password manager"
    );
    assert_eq!(key_entry.as_deref(), Some(entry.as_str()));
    let Resolved::Ready(keys, _) = upload_key::resolve(&passwords, &anna.db)
        .await
        .expect("resolve")
    else {
        panic!("the key is ready");
    };
    assert_eq!(signer, keys.public_key().to_hex());
    Server::new(&blossom.url, &SystemResolver)
        .await
        .expect("server")
        .delete(&keys, &blob, &SystemResolver)
        .await
        .expect("the server took the blob from the upload key, so it deletes it for that key");

    passwords
        .trash_targets(
            &Caller::User,
            vec![crate::passwords::model::Target {
                kind: crate::passwords::model::TargetKind::Item,
                id: entry,
            }],
        )
        .await
        .expect("the user trashes the entry");
    let refused = send_file(&anna, &ben, "Noch einer", PLAN).await;
    assert!(
        matches!(&refused, Err(HaexError::InvalidInput { reason }) if reason == "chat.file.keyMissing"),
        "a key that is set but gone must not fall back to a fresh one: {refused:?}"
    );
}
