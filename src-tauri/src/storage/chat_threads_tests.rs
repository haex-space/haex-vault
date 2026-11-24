//! Operator 2026-11-06: a model picked while a session is open becomes that session's model, so
//! its older model no longer wins before the next message (typed or spoken).

use uuid::Uuid;

use super::super::query;
use super::{
    get_thread, insert_thread, mark_foreign_once, memory_foreign_at, memory_toc, set_thread_model,
    take_memory_toc, ChatThread,
};
use crate::chat::default_model::choose_model;
use crate::passwords::test_support::open_test_vault as vault;

#[test]
fn a_pick_becomes_the_model_of_the_open_session() {
    let (_tmp, db) = vault();
    let thread_id = Uuid::new_v4();
    let provider = Uuid::new_v4();
    let claude = format!("{provider}:claude");
    db.write(|tx| {
        insert_thread(
            tx,
            &ChatThread {
                id: thread_id,
                title: "Wetter".into(),
                last_provider_id: None,
                last_model_id: Some("gemma-4-e4b-instruct-q4_k_m".into()),
                created_at: 1,
                updated_at: 2,
                origin: None,
            },
        )
    })
    .expect("thread");

    let changed = db
        .write(|tx| set_thread_model(tx, thread_id, Some(provider), &claude))
        .expect("set");
    assert_eq!(changed, 1);

    let thread = query::read(&db, |r| get_thread(r, thread_id))
        .expect("read")
        .expect("thread");
    assert_eq!(thread.last_model_id.as_deref(), Some(claude.as_str()));
    assert_eq!(thread.last_provider_id, Some(provider));
    assert_eq!(thread.title, "Wetter", "the title stays");
    assert_eq!(thread.updated_at, 2, "a pick is not a message");

    // The session now resolves to the pick, not to the model it used before.
    let resolved = choose_model(
        [thread.last_model_id.clone(), Some(claude.clone())],
        vec!["gemma-4-e4b-instruct-q4_k_m".into()],
        vec![claude.clone()],
    );
    assert_eq!(resolved.model_id.as_deref(), Some(claude.as_str()));
}

#[test]
fn a_local_pick_clears_the_provider_of_the_session() {
    let (_tmp, db) = vault();
    let thread_id = Uuid::new_v4();
    db.write(|tx| {
        insert_thread(
            tx,
            &ChatThread {
                id: thread_id,
                title: "Hallo".into(),
                last_provider_id: Some(Uuid::new_v4()),
                last_model_id: Some("provider:model".into()),
                created_at: 1,
                updated_at: 1,
                origin: None,
            },
        )
    })
    .expect("thread");
    db.write(|tx| set_thread_model(tx, thread_id, None, "gemma-4-e4b-instruct-q4_k_m"))
        .expect("set");
    let thread = query::read(&db, |r| get_thread(r, thread_id))
        .expect("read")
        .expect("thread");
    assert_eq!(thread.last_provider_id, None);
    assert_eq!(
        thread.last_model_id.as_deref(),
        Some("gemma-4-e4b-instruct-q4_k_m")
    );
}

/// Spec 097 T038: the table of contents and the mark of foreign content are written once.
#[test]
fn the_memory_columns_of_a_thread_are_written_once() {
    let (_tmp, db) = vault();
    let id = Uuid::new_v4();
    db.write(|tx| {
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
    .expect("insert");
    let read = |db: &haex_crdt::Database| {
        query::read(db, |q| Ok((memory_toc(q, id)?, memory_foreign_at(q, id)?))).expect("read")
    };
    assert_eq!(read(&db), (None, None));
    let first = db
        .write(|tx| take_memory_toc(tx, id, "- A: a (the user, 2026-11-20)"))
        .expect("take");
    let second = db
        .write(|tx| take_memory_toc(tx, id, "- B: b"))
        .expect("take");
    assert_eq!(first.as_deref(), Some("- A: a (the user, 2026-11-20)"));
    assert_eq!(second, first, "the snapshot is taken once");
    db.write(|tx| mark_foreign_once(tx, id, 5)).expect("mark");
    db.write(|tx| mark_foreign_once(tx, id, 9)).expect("mark");
    assert_eq!(read(&db).1, Some(5), "the first foreign content counts");
    let listed = query::read(&db, |q| get_thread(q, id))
        .expect("read")
        .expect("found");
    assert_eq!(listed.title, "Mails", "the thread reads as before");
}
