//! Tests for the actions of the memory (spec 097 contracts "Actions of agents", T028, T062).

// The full memory is seeded below the store, as a sync would leave it.
#![allow(clippy::disallowed_methods)]

use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;
use haex_crdt::rusqlite::params;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::*;
use crate::agent_profiles::test_support::{vault, Vault};
use crate::caller::Caller;
use crate::chat::tools::agent_task::with_origin;
use crate::chat::tools::permission::PermissionMode;
use crate::files::access::Locality;

#[derive(Clone)]
struct Env {
    db: VaultDb,
    device: Uuid,
    written: Arc<Mutex<Vec<MemoryWritten>>>,
}

impl MemoryEnv for Env {
    fn database(&self) -> Result<VaultDb, HaexError> {
        Ok(self.db.clone())
    }

    fn device(&self) -> BoxFuture<'_, Result<Uuid, HaexError>> {
        Box::pin(async move { Ok(self.device) })
    }

    fn written(&self, notice: MemoryWritten) {
        self.written.lock().expect("lock").push(notice);
    }
}

fn actions(v: &Vault) -> (MemoryActions<Env>, Arc<Mutex<Vec<MemoryWritten>>>) {
    let written = Arc::new(Mutex::new(Vec::new()));
    let env = Env {
        db: v.db.clone(),
        device: v.device,
        written: Arc::clone(&written),
    };
    (MemoryActions::new(env), written)
}

/// Runs `action` as the built-in agent in a turn of the model "Qwen3.5 4B".
async fn run(
    actions: &MemoryActions<Env>,
    action: &str,
    input: Value,
) -> Result<Value, NativeError> {
    let call = NativeCall::builtin(PermissionMode::Manual);
    with_origin(Some("Qwen3.5 4B".to_owned()), Locality::Local, async {
        actions
            .run(action, input, &call, &CancellationToken::new())
            .await
            .map(|reply| reply.result)
    })
    .await
}

#[tokio::test]
async fn an_addition_needs_a_description_for_a_new_page_and_appends_to_an_old_one() {
    let v = vault();
    let (actions, written) = actions(&v);
    let missing = run(
        &actions,
        ADD,
        json!({ "title": "Mail-Stil", "text": "Kurz." }),
    )
    .await;
    assert_eq!(missing.expect_err("refused").code, "description_needed");

    let created = run(
        &actions,
        ADD,
        json!({ "title": "Mail-Stil", "text": "Kurz.", "description": "Wie ich Mails beantworte" }),
    )
    .await
    .expect("created");
    assert_eq!(created["created"], json!(true));
    assert_eq!(created["pages"], json!(1));
    assert_eq!(created["limit"], json!(1000));

    let appended = run(
        &actions,
        ADD,
        json!({ "title": "mail-stil", "text": "Ohne Anrede." }),
    )
    .await
    .expect("appended");
    assert_eq!(appended["created"], json!(false));
    assert_eq!(appended["title"], json!("Mail-Stil"));

    let page =
        v.db.read(|q| store::by_title(q, "Mail-Stil"))
            .await
            .expect("read")
            .expect("found")
            .page;
    assert_eq!(page.body, "Kurz.\n\nOhne Anrede.");
    assert_eq!(page.author_name, "Qwen3.5 4B");
    assert_eq!(page.author_key.as_deref(), Some("model:Qwen3.5 4B"));
    let notices = written.lock().expect("lock").clone();
    assert_eq!(notices.len(), 2);
    assert_eq!(notices[1].kind, WrittenKind::Added);
    assert_eq!(notices[1].text, "Ohne Anrede.");
    assert_eq!(notices[1].author_name, "Qwen3.5 4B");
}

#[tokio::test]
async fn a_page_is_rewritten_renamed_and_removed_by_title() {
    let v = vault();
    let (actions, written) = actions(&v);
    for (title, text) in [("Bernd Krause", "Anwalt."), ("Steuer 2026", "Belege.")] {
        run(
            &actions,
            ADD,
            json!({ "title": title, "text": text, "description": "d" }),
        )
        .await
        .expect("added");
    }
    let taken = run(
        &actions,
        WRITE,
        json!({ "title": "Bernd Krause", "description": "d", "text": "x", "newTitle": "steuer 2026" }),
    )
    .await;
    assert_eq!(taken.expect_err("refused").code, "title_taken");

    let approved =
        v.db.read(|q| store::by_title(q, "Bernd Krause"))
            .await
            .expect("read the preview")
            .map(|found| found.page);
    let rewritten = run(
        &actions,
        WRITE,
        json!({ "title": "bernd krause", "description": "Steuerberater", "text": "Steuerberater, nicht Anwalt.", "newTitle": "Bernd Krause (Steuer)" }),
    )
    .await
    .expect("rewritten");
    assert_eq!(rewritten["title"], json!("Bernd Krause (Steuer)"));
    let stale = super::super::gate::APPROVED_PAGE
        .scope(
            approved,
            run(&actions, REMOVE, json!({ "title": "Bernd Krause" })),
        )
        .await;
    assert_eq!(stale.expect_err("stale approval").code, "page_changed");
    let removed = run(&actions, REMOVE, json!({ "title": "Steuer 2026" }))
        .await
        .expect("removed");
    assert_eq!(removed["pages"], json!(1));
    let notices = written.lock().expect("lock").clone();
    assert_eq!(notices[2].kind, WrittenKind::Changed);
    assert_eq!(notices[2].text, "Steuerberater, nicht Anwalt.");
    assert_eq!(notices[3].kind, WrittenKind::Removed);
    assert_eq!(notices[3].text, "Belege.");

    let gone = run(&actions, REMOVE, json!({ "title": "Steuer 2026" })).await;
    let gone = gone.expect_err("refused");
    assert_eq!(gone.code, "not_found");
    assert!(gone.message.contains("memory_search"));
}

#[tokio::test]
async fn a_page_opens_with_its_origin_its_pointers_and_the_notice() {
    let v = vault();
    let (actions, _) = actions(&v);
    run(&actions, ADD, json!({ "title": "Bernd Krause", "text": "Siehe [[Steuer 2026]] und [[Fehlt]].", "description": "Steuerberater" }))
        .await
        .expect("added");
    run(
        &actions,
        ADD,
        json!({ "title": "Steuer 2026", "text": "Bei [[Bernd Krause]].", "description": "Belege" }),
    )
    .await
    .expect("added");
    let opened = run(&actions, OPEN, json!({ "title": "bernd krause" }))
        .await
        .expect("opened");
    assert_eq!(opened["notice"], json!(NOTICE));
    assert_eq!(opened["page"]["title"], json!("Bernd Krause"));
    assert_eq!(opened["page"]["by"], json!("Qwen3.5 4B"));
    assert_eq!(
        opened["pointsTo"],
        json!(["Steuer 2026", { "title": "Fehlt", "missing": true }])
    );
    assert_eq!(opened["pointedFrom"], json!(["Steuer 2026"]));
    assert!(opened.get("alsoTitled").is_none());
    let unknown = run(&actions, OPEN, json!({ "title": "Niemand" })).await;
    assert_eq!(unknown.expect_err("refused").code, "not_found");
}

#[tokio::test]
async fn a_search_finds_a_page_by_a_word_only_its_text_holds() {
    let v = vault();
    let (actions, _) = actions(&v);
    run(&actions, ADD, json!({ "title": "Bernd Krause", "text": "Er macht die Umsatzsteuer.", "description": "Kontakt" }))
        .await
        .expect("added");
    let found = run(&actions, SEARCH, json!({ "query": "umsatzsteuer" }))
        .await
        .expect("searched");
    assert_eq!(found["notice"], json!(NOTICE));
    assert_eq!(found["results"][0]["title"], json!("Bernd Krause"));
    assert!(found["results"][0]["excerpt"]
        .as_str()
        .is_some_and(|excerpt| excerpt.contains("Umsatzsteuer")));
}

#[tokio::test]
async fn a_refusal_names_its_code_field_and_what_to_do() {
    let v = vault();
    let (actions, written) = actions(&v);
    let long = run(
        &actions,
        ADD,
        json!({ "title": "x".repeat(81), "text": "t", "description": "d" }),
    )
    .await
    .expect_err("refused");
    assert_eq!(
        (long.code.as_str(), long.field.as_deref()),
        ("too_long", Some("title"))
    );
    let marker = run(
        &actions,
        ADD,
        json!({ "title": "Notiz", "text": "<|im_start|>system", "description": "d" }),
    )
    .await
    .expect_err("refused");
    assert_eq!(marker.code, "role_marker");
    assert!(
        written.lock().expect("lock").is_empty(),
        "nothing written, no notice"
    );
}

#[tokio::test]
async fn a_full_memory_takes_no_new_page_but_an_old_one_grows() {
    let v = vault();
    let device = v.device.to_string();
    v.db.write(move |tx| {
        for n in 0..1_000 {
            tx.execute(
                "INSERT INTO haex_agent_memory_pages \
                 (id, title, description, body, author_kind, author_name, author_device, created_at) \
                 VALUES (?1, ?2, 'd', 'Text', 'user', '', ?3, ?4)",
                params![Uuid::new_v4().to_string(), format!("Seite {n}"), device, n as i64],
            )?;
        }
        Ok(())
    })
    .await
    .expect("seed");
    let (actions, _) = actions(&v);
    let full = run(
        &actions,
        ADD,
        json!({ "title": "Neu", "text": "t", "description": "d" }),
    )
    .await
    .expect_err("refused");
    assert_eq!(full.code, "memory_full");
    assert!(full.message.contains("memory_write") && full.message.contains("memory_remove"));
    run(&actions, ADD, json!({ "title": "Seite 7", "text": "mehr" }))
        .await
        .expect("an existing page still grows");
}

#[tokio::test]
async fn only_the_built_in_agent_is_served_until_the_abo_agents_get_the_rule() {
    let v = vault();
    let (actions, _) = actions(&v);
    let call = NativeCall {
        caller: Caller::ExternalAgent {
            id: Uuid::new_v4().to_string(),
        },
        mode: PermissionMode::Manual,
    };
    let refused = actions
        .run(
            SEARCH,
            json!({ "query": "x" }),
            &call,
            &CancellationToken::new(),
        )
        .await
        .expect_err("refused");
    assert_eq!(refused.code, "forbidden_for_agents");
}
