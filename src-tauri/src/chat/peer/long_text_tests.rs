//! Long texts (spec 072 FR-012, research R2 as amended 2026-11-20, T123, T124): a text that fits one
//! wrap goes as a message of its own, a longer one as a file with a preview that arrives in full, a
//! note takes any text up to the bound, and only the bound refuses.

use super::*;
use crate::chat::peer::envelope::{BlobRef, FileAttachment, INLINE_MIME, MAX_TEXT_CHARS};
use crate::chat::peer::files::test_server::BlossomTestServer;
use crate::chat::peer::test_support::{mock_relay, two_vaults, ChatDevice};
use crate::chat::peer::{ids, outbox, receive, settings, store};
use crate::storage::query::Query;

/// `count` different words: text that is not one repeated character.
fn words(count: usize) -> String {
    (0..count)
        .map(|index| format!("wort{index}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn open_direct(from: &ChatDevice, to: &ChatDevice) -> Uuid {
    let (own, peer) = (from.me.vault_hex.clone(), to.me.vault_hex.clone());
    from.db
        .write_blocking(move |tx| Ok(store::open_direct(tx, &own, &peer, now_ms())?))
        .expect("open")
}

async fn send_to(from: &ChatDevice, to: &ChatDevice, event: &str, text: &str) -> Result<Uuid> {
    let thread = open_direct(from, to);
    send_text(&from.db, &from.session, thread, event, text).await
}

fn queued(device: &ChatDevice) -> usize {
    device
        .db
        .read_blocking(|r| Ok(outbox::due(r, i64::MAX - 1)?))
        .expect("outbox")
        .len()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_text_over_the_old_limit_that_fits_one_wrap_is_a_message_of_its_own() {
    let blossom = BlossomTestServer::start().await;
    let (anna, ben) = two_vaults(&[]).await;
    anna.enable_files(&blossom.url, None);
    let text = words(1_000);
    assert!(text.chars().count() > 4_000);

    send_to(&anna, &ben, "e1", &text).await.expect("send");

    let sent = anna.messages_with(&ben);
    assert_eq!(sent[0].text.as_deref(), Some(text.as_str()));
    assert!(sent[0].attachments.is_empty());
    assert!(blossom.log().is_empty(), "nothing was uploaded");
    assert_eq!(queued(&anna), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_long_text_goes_as_a_file_with_a_preview_and_arrives_in_full() {
    let (_relay, url) = mock_relay().await;
    let blossom = BlossomTestServer::start().await;
    let (anna, ben) = two_vaults(&[url]).await;
    anna.enable_files(&blossom.url, None);
    ben.enable_files(&blossom.url, None);
    anna.start().await;
    ben.start().await;
    let text = words(20_000);
    assert!(text.chars().count() > 100_000);

    send_to(&anna, &ben, "e1", &text).await.expect("send");

    let at_anna = anna.messages_with(&ben);
    let preview = envelope::preview(&text);
    assert_eq!(at_anna[0].text.as_deref(), Some(preview.as_str()));
    let file = &at_anna[0].attachments[0];
    assert!(file.inline);
    assert_eq!(file.name.as_deref(), Some("message.txt"));
    assert_eq!(file.mime.as_deref(), Some(INLINE_MIME));
    assert_eq!(file.size, text.len() as i64);
    assert_eq!(
        file.state.as_deref(),
        Some("stored"),
        "the sender has it at once"
    );
    let own_files = anna.session.files.get().expect("files").clone();
    assert_eq!(own_files.text(&file.id).await.expect("own text"), text);

    let at_ben = ben
        .wait_for(&anna, "the text at Ben", |messages| {
            messages
                .first()
                .and_then(|m| m.attachments.first())
                .and_then(|a| a.state.as_deref())
                == Some("stored")
        })
        .await;
    assert_eq!(at_ben[0].text.as_deref(), Some(preview.as_str()));
    let received = &at_ben[0].attachments[0];
    assert!(received.inline);
    assert_eq!(received.id, file.id, "both name the same attachment");
    let ben_files = ben.session.files.get().expect("files").clone();
    assert_eq!(ben_files.text(&received.id).await.expect("text"), text);

    let snippet = b"wort12345 wort12346";
    let on_server: Vec<u8> = blossom.log().into_iter().flat_map(|l| l.body).collect();
    assert!(
        !on_server.windows(snippet.len()).any(|w| w == snippet),
        "the server only saw ciphertext"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_long_text_stays_an_inline_file_when_ordinary_files_are_attached() {
    let blossom = BlossomTestServer::start().await;
    let (anna, ben) = two_vaults(&[]).await;
    anna.enable_files(&blossom.url, None);
    let thread = open_direct(&anna, &ben);
    let text = words(20_000);
    let (preview, inline_text) = prepare_with_files(&anna.db, &anna.session, thread, &text)
        .await
        .expect("prepare long text");
    let mut files = vec![super::super::files::transfers::Picked::in_memory(
        "photo.png",
        "image/png",
        vec![1, 2, 3],
        false,
    )];
    files.push(inline_text.expect("long text becomes a file"));

    anna.session
        .files
        .get()
        .expect("files")
        .send(&anna.me, thread, "e1", &preview, files)
        .await
        .expect("send files and long text");

    let sent = anna.messages_with(&ben);
    assert_eq!(sent[0].text.as_deref(), Some(preview.as_str()));
    assert_eq!(sent[0].attachments.len(), 2);
    assert!(!sent[0].attachments[0].inline);
    assert!(sent[0].attachments[1].inline);
    assert_eq!(sent[0].attachments[1].name.as_deref(), Some("message.txt"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_retry_with_the_same_key_sends_a_long_text_once() {
    let blossom = BlossomTestServer::start().await;
    let (anna, ben) = two_vaults(&[]).await;
    anna.enable_files(&blossom.url, None);
    let text = words(20_000);

    let first = send_to(&anna, &ben, "e1", &text).await.expect("send");
    let retry = send_to(&anna, &ben, "e1", &text).await.expect("retry");

    assert_eq!(first, retry);
    assert_eq!(anna.messages_with(&ben).len(), 1);
    assert_eq!(queued(&anna), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_a_server_only_a_text_that_needs_a_file_is_refused() {
    let blossom = BlossomTestServer::start().await;
    let (anna, ben) = two_vaults(&[]).await;
    anna.enable_files(&blossom.url, None);
    anna.db
        .write_blocking(|tx| Ok(settings::set_blossom_server(tx, None)?))
        .expect("no server");

    let refused = send_to(&anna, &ben, "e1", &words(20_000))
        .await
        .expect_err("needs a file");
    assert!(
        refused.to_string().contains("chat.file.noServer"),
        "{refused}"
    );
    assert!(anna.messages_with(&ben).is_empty(), "no message queued");
    assert_eq!(queued(&anna), 0);

    send_to(&anna, &ben, "e2", &words(1_000))
        .await
        .expect("a text that fits needs no server");
    assert_eq!(anna.messages_with(&ben).len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_text_over_the_bound_is_refused_before_anything_happens() {
    let blossom = BlossomTestServer::start().await;
    let (anna, ben) = two_vaults(&[]).await;
    anna.enable_files(&blossom.url, None);

    let refused = send_to(&anna, &ben, "e1", &"x".repeat(MAX_TEXT_CHARS + 1))
        .await
        .expect_err("over the bound");

    assert!(
        refused.to_string().contains("chat.error.tooLong"),
        "{refused}"
    );
    assert!(anna.messages_with(&ben).is_empty());
    assert_eq!(queued(&anna), 0);
    assert!(blossom.log().is_empty(), "nothing was uploaded");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_note_takes_any_text_up_to_the_bound_without_a_file() {
    let anna = ChatDevice::genesis(&[]);
    let own = anna.me.vault_hex.clone();
    let thread = anna
        .db
        .write_blocking(move |tx| Ok(store::open_note(tx, &own, now_ms())?))
        .expect("open notes");

    let text = "x".repeat(MAX_TEXT_CHARS);
    send_text(&anna.db, &anna.session, thread, "n1", &text)
        .await
        .expect("a note at the bound");
    let refused = send_text(&anna.db, &anna.session, thread, "n2", &format!("{text}x"))
        .await
        .expect_err("over the bound");
    assert!(
        refused.to_string().contains("chat.error.tooLong"),
        "{refused}"
    );

    let own = anna.me.vault_hex.clone();
    let notes = anna
        .db
        .read_blocking(move |r| {
            Ok(crate::chat::peer::messages::list(
                r,
                &own,
                thread,
                Default::default(),
            )?)
        })
        .expect("notes");
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].text.as_deref().map(str::len), Some(MAX_TEXT_CHARS));
    assert!(notes[0].attachments.is_empty());
    assert_eq!(queued(&anna), 0, "a note is never delivered");
}

fn marked(size: u64, mime: &str, inline: bool) -> envelope::Attachment {
    envelope::Attachment::File(FileAttachment {
        name: "message.txt".into(),
        mime: mime.into(),
        size,
        sha256: "aa".into(),
        blossom: BlobRef {
            url: "https://blossom.example/bb".into(),
            sha256: "bb".into(),
            key: "00".repeat(32),
        },
        inline,
    })
}

#[tokio::test]
async fn the_receiver_honours_the_marker_only_for_a_text_file_of_a_size_it_reads() {
    let anna = ChatDevice::genesis(&[]);
    let message = Uuid::new_v4();
    let attachments = [
        marked(5, INLINE_MIME, true),
        marked(5, INLINE_MIME, false),
        marked(5, "application/pdf", true),
        marked(envelope::MAX_INLINE_FILE_BYTES + 1, INLINE_MIME, true),
        marked(envelope::MAX_INLINE_FILE_BYTES, INLINE_MIME, true),
    ];
    anna.db
        .write_blocking(move |tx| Ok(receive::insert_attachments(tx, message, &attachments)?))
        .expect("insert");

    let flags: Vec<bool> = (0..5)
        .map(|index| {
            let id = ids::attachment_id(message, index).to_string();
            anna.db
                .read_blocking(move |r| {
                    r.query_row(
                        "SELECT inline FROM chat_attachments WHERE id = ?1",
                        haex_crdt::rusqlite::params![id],
                        |row| row.get::<_, i64>(0),
                    )
                })
                .expect("read")
                .expect("row")
                != 0
        })
        .collect();
    assert_eq!(flags, [true, false, false, false, true]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_text_of_a_file_is_read_only_for_an_inline_file_with_a_copy_that_is_text() {
    let blossom = BlossomTestServer::start().await;
    let anna = ChatDevice::genesis(&[]);
    let files = anna.enable_files(&blossom.url, None);
    let message = Uuid::new_v4();
    anna.db
        .write_blocking(move |tx| {
            Ok(receive::insert_attachments(
                tx,
                message,
                &[marked(5, INLINE_MIME, true), marked(5, INLINE_MIME, false)],
            )?)
        })
        .expect("insert");
    let (inline, ordinary) = (
        ids::attachment_id(message, 0).to_string(),
        ids::attachment_id(message, 1).to_string(),
    );

    let missing = files.text(&inline).await.expect_err("no copy yet");
    assert!(missing.to_string().contains("chat text file"), "{missing}");

    files
        .store
        .put(&inline, &[0xff, 0xfe, 0xfd])
        .await
        .expect("put");
    let damaged = files.text(&inline).await.expect_err("not UTF-8");
    assert!(
        damaged.to_string().contains("chat.file.damaged"),
        "{damaged}"
    );

    files
        .store
        .put(&ordinary, b"text of an ordinary file")
        .await
        .expect("put");
    let refused = files.text(&ordinary).await.expect_err("not inline");
    assert!(refused.to_string().contains("chat text"), "{refused}");
    let unknown = files
        .text(&Uuid::new_v4().to_string())
        .await
        .expect_err("unknown");
    assert!(unknown.to_string().contains("chat text"), "{unknown}");

    files.store.put(&inline, b"Hallo Welt").await.expect("put");
    assert_eq!(files.text(&inline).await.expect("text"), "Hallo Welt");
}
