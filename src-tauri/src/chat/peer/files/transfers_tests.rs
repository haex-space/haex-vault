//! Files between two vaults over a Nostr-Relay and a Blossom-Server in the process (spec 072 T059,
//! US3 scenarios 1, 2, 5, 6, 8): the receiver open or closed, above the download limit, refused by
//! the server or without one, a blob that is gone, and the receiver's second device downloading
//! itself.

use std::sync::{Arc, Mutex};

use super::*;
use crate::chat::peer::files::test_server::BlossomTestServer;
use crate::chat::peer::messages::PeerMessage;
use crate::chat::peer::test_support::{mock_relay, two_vaults, ChatDevice};

const PLAN: &[u8] = b"%PDF-1.7 the plan for saturday, never in plaintext on a server";

fn picked(bytes: &[u8]) -> Picked {
    Picked {
        name: "Plan.pdf".into(),
        mime: "application/pdf".into(),
        bytes: bytes.to_vec(),
    }
}

async fn send_file(from: &ChatDevice, to: &ChatDevice, text: &str, bytes: &[u8]) -> Result<Uuid> {
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
    let sent = files
        .send(&from.me, thread, &event, text, vec![picked(bytes)])
        .await;
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
