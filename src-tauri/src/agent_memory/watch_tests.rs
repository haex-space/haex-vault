//! Tests for the reader barrier and the mark of an Abo-Agent's own tools (spec 097 research R7,
//! T066, T072).

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use uuid::Uuid;

use super::*;
use crate::agent_profiles::test_support::{vault, Vault};
use crate::storage::chat_threads::{insert_thread, memory_foreign_at, ChatThread};

async fn thread(v: &Vault) -> Uuid {
    let id = Uuid::new_v4();
    v.db.write(move |tx| {
        insert_thread(
            tx,
            &ChatThread {
                id,
                title: "Codex".into(),
                last_provider_id: None,
                last_model_id: None,
                created_at: 1,
                updated_at: 1,
                origin: None,
            },
        )
    })
    .await
    .expect("thread");
    id
}

#[tokio::test]
async fn a_fence_waits_until_every_line_written_before_it_was_judged() {
    let v = vault();
    let thread = thread(&v).await;
    let (mut agent, output) = tokio::io::duplex(1024);
    agent
        .write_all(b"{\"web\":true}\n{\"text\":\"done\"}\n")
        .await
        .expect("write");
    let mut lines = BufReader::new(output).lines();
    let mut watch = Watch::start(Some(thread), Some(v.db.clone()));
    // The server's call arrives before the reader has read anything.
    let mut fence = tokio::spawn(fence(thread, FENCE_WAIT));
    tokio::task::yield_now().await;

    let first = watch.next_line(&mut lines).await.expect("read");
    assert_eq!(first.as_deref(), Some("{\"web\":true}"));
    assert!(!fence.is_finished(), "not before the first line is judged");
    watch.saw_foreign().await;
    let second = watch.next_line(&mut lines).await.expect("read");
    assert_eq!(second.as_deref(), Some("{\"text\":\"done\"}"));
    assert!(!fence.is_finished(), "not before the second line is judged");

    let acknowledged = tokio::select! {
        acknowledged = &mut fence => acknowledged.expect("join"),
        _ = watch.next_line(&mut lines) => panic!("the agent wrote nothing more"),
    };
    assert!(acknowledged);
    let marked =
        v.db.read(move |q| memory_foreign_at(q, thread))
            .await
            .expect("read");
    assert!(
        marked.is_some(),
        "the mark is written before the fence is answered"
    );
}

#[tokio::test]
async fn a_fence_without_a_reader_fails_and_a_dropped_reader_ends_its_fences() {
    assert!(!fence(Uuid::new_v4(), Duration::from_millis(50)).await);
    let v = vault();
    let thread = thread(&v).await;
    let watch = Watch::start(Some(thread), Some(v.db.clone()));
    let waiting = tokio::spawn(fence(thread, FENCE_WAIT));
    tokio::task::yield_now().await;
    drop(watch);
    assert!(!waiting.await.expect("join"));
    assert!(!fence(thread, Duration::from_millis(50)).await);
}

#[tokio::test]
async fn an_unrecorded_foreign_mark_fails_closed_until_a_retry_stores_it() {
    let v = vault();
    let thread = thread(&v).await;
    let mut watch = Watch::start(Some(thread), Some(v.db.clone()));
    mark_unrecorded(thread);

    assert!(!fence(thread, Duration::from_millis(50)).await);
    assert!(retry_unrecorded(&v.db, thread).await);
    assert!(!is_unrecorded(thread));
    assert!(v
        .db
        .read(move |q| memory_foreign_at(q, thread))
        .await
        .expect("read the mark")
        .is_some());

    let mut fence = tokio::spawn(fence(thread, FENCE_WAIT));
    tokio::task::yield_now().await;
    let (_agent, output) = tokio::io::duplex(64);
    let mut lines = BufReader::new(output).lines();
    tokio::select! {
        acknowledged = &mut fence => assert!(acknowledged.expect("join")),
        _ = watch.next_line(&mut lines) => panic!("the agent wrote nothing"),
    }
}

#[tokio::test]
async fn a_pass_without_a_thread_is_not_watched() {
    let (_agent, output) = tokio::io::duplex(64);
    let mut lines = BufReader::new(output).lines();
    let mut watch = Watch::start(None, None);
    watch.saw_foreign().await;
    let read = tokio::time::timeout(Duration::from_millis(50), watch.next_line(&mut lines)).await;
    assert!(read.is_err(), "it just waits for the next line");
}
