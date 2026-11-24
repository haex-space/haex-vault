//! Tests for the memory part of a turn (spec 097 FR-011, FR-015, SC-007 note, T030, T045).

use uuid::Uuid;

use super::super::store::{add_page, NewPage};
use super::super::toc::REMEMBER;
use super::super::Author;
use super::*;
use crate::agent_profiles::test_support::{vault, Vault};
use crate::storage::chat_threads::{insert_thread, ChatThread};

async fn thread(v: &Vault) -> Uuid {
    let id = Uuid::new_v4();
    v.db.write(move |tx| {
        insert_thread(
            tx,
            &ChatThread {
                id,
                title: "Mails".into(),
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

async fn page(v: &Vault, title: &str, at: i64) {
    let (author, title) = (Author::user(v.device), title.to_owned());
    v.db.write(move |tx| {
        Ok(add_page(
            tx,
            &author,
            at,
            &NewPage {
                title: &title,
                description: "d",
                body: "",
            },
        )
        .expect("added"))
    })
    .await
    .expect("write");
}

async fn turn(v: &Vault, id: Uuid, level: MemoryLevel) -> Option<String> {
    v.db.write(move |tx| prepare(tx, id, level, Some(32_768), Some("model:p1:qwen")))
        .await
        .expect("prepare")
}

#[tokio::test]
async fn a_running_conversation_keeps_the_block_of_its_start() {
    let v = vault();
    page(&v, "Mail-Stil", 10).await;
    let id = thread(&v).await;
    let first = turn(&v, id, MemoryLevel::ReadWrite).await.expect("block");
    assert!(first.contains("- Mail-Stil: d"));
    page(&v, "Bernd Krause", 20).await;
    let second = turn(&v, id, MemoryLevel::ReadWrite).await.expect("block");
    assert_eq!(
        first, second,
        "a write does not change the running conversation's prompt"
    );
    let other = thread(&v).await;
    let fresh = turn(&v, other, MemoryLevel::ReadWrite)
        .await
        .expect("block");
    assert!(
        fresh.contains("- Bernd Krause: d"),
        "a new conversation has it"
    );
}

#[tokio::test]
async fn the_level_decides_what_the_model_is_given() {
    let v = vault();
    page(&v, "Mail-Stil", 10).await;
    let id = thread(&v).await;
    assert_eq!(turn(&v, id, MemoryLevel::Off).await, None);
    let read = turn(&v, id, MemoryLevel::Read).await.expect("block");
    assert!(
        !read.contains(REMEMBER),
        "a reader is not asked to remember"
    );
    let write = turn(&v, id, MemoryLevel::ReadWrite).await.expect("block");
    assert!(write.contains(REMEMBER));
}

#[tokio::test]
async fn a_model_without_the_right_leaves_the_snapshot_for_one_that_has_it() {
    let v = vault();
    page(&v, "Mail-Stil", 10).await;
    let id = thread(&v).await;
    assert_eq!(
        turn(&v, id, MemoryLevel::Off).await,
        None,
        "first turn, memory off"
    );
    page(&v, "Bernd Krause", 20).await;
    let later = turn(&v, id, MemoryLevel::Read).await.expect("block");
    assert!(later.contains("Mail-Stil"));
    assert!(
        !later.contains("Bernd Krause"),
        "the snapshot is the one from the conversation's start"
    );
}

#[tokio::test]
async fn an_abo_agent_is_given_the_level_of_its_model_s_profile() {
    let v = vault();
    let model = "6f3c0a52-0a47-4c0a-9a61-5d2d1a5b7e01:gpt-6.2-sol";
    assert_eq!(
        delegate_level(&v.db, model).await,
        MemoryLevel::ReadWrite,
        "a model in Standard reads and writes from the factory"
    );
    crate::agent_profiles::test_support::edit_standard(&v.db, |rights| {
        rights.memory = MemoryLevel::Read;
    });
    assert_eq!(delegate_level(&v.db, model).await, MemoryLevel::Read);
}
