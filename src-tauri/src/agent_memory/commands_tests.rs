//! Tests for the commands of "KI → Gedächtnis" (spec 097 contracts, T015): the inner functions that
//! take a `Query` or the CRDT transaction, not the Tauri wrappers.

// The doubled title and the unknown device are seeded below the store, as a sync would bring them.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use uuid::Uuid;

use super::super::limits::MAX_PAGES;
use super::super::store::NewPage;
use super::super::Author;
use super::*;
use crate::agent_profiles::test_support::{vault, Vault};

async fn add(v: &Vault, title: &str, description: &str, body: &str, now: i64) -> AgentMemoryPage {
    let (author, title, description, body) = (
        Author::user(v.device),
        title.to_owned(),
        description.to_owned(),
        body.to_owned(),
    );
    v.db.write(move |tx| {
        add_in(
            tx,
            &author,
            now,
            &NewPage {
                title: &title,
                description: &description,
                body: &body,
            },
        )
    })
    .await
    .expect("write")
    .expect("added")
}

async fn listed(v: &Vault, query: Option<&str>) -> AgentMemoryList {
    let query = query.map(str::to_owned);
    v.db.read(move |q| list_view(q, query.as_deref()))
        .await
        .expect("read")
}

#[tokio::test]
async fn the_list_has_every_page_with_its_origin_and_the_limit() {
    let v = vault();
    add(
        &v,
        "Bernd Krause",
        "Steuerberater",
        "Siehe [[Steuer 2026]]",
        10,
    )
    .await;
    add(&v, "Mail-Stil", "Wie ich Mails beantworte", "Kurz.", 20).await;
    let list = listed(&v, None).await;
    assert_eq!(list.count, 2);
    assert_eq!(list.limit, MAX_PAGES);
    let titles: Vec<&str> = list.pages.iter().map(|p| p.title.as_str()).collect();
    assert_eq!(titles, vec!["Mail-Stil", "Bernd Krause"]);
    let item = &list.pages[1];
    assert_eq!(item.author_kind, "user");
    assert_eq!(item.author_name, "");
    assert_eq!(item.created_at, 10);
    assert!(!item.same_title);
}

#[tokio::test]
async fn a_query_lists_the_matches_best_first() {
    let v = vault();
    add(
        &v,
        "Briefe",
        "Schluss",
        "Mein Steuerberater bekommt Briefe.",
        10,
    )
    .await;
    add(&v, "Steuerberater", "Kontakt", "", 20).await;
    add(&v, "Mail-Stil", "Kurz", "", 30).await;
    let list = listed(&v, Some("steuerberater")).await;
    let titles: Vec<&str> = list.pages.iter().map(|p| p.title.as_str()).collect();
    assert_eq!(titles, vec!["Steuerberater", "Briefe"]);
    assert_eq!(list.count, 3, "the count is of the whole memory");
}

#[tokio::test]
async fn a_page_shows_its_text_and_its_pointers_both_ways() {
    let v = vault();
    let bernd = add(
        &v,
        "Bernd Krause",
        "Steuerberater",
        "Siehe [[Steuer 2026]] und [[Fehlt]].",
        10,
    )
    .await;
    let steuer = add(&v, "Steuer 2026", "Belege", "Bei [[bernd krause]].", 20).await;
    assert_eq!(bernd.body, "Siehe [[Steuer 2026]] und [[Fehlt]].");
    assert_eq!(
        bernd.points_to[0].id, None,
        "when the first page was written the second did not exist yet"
    );
    let id = bernd.id.clone();
    let shown =
        v.db.read(move |q| page_view(q, &id))
            .await
            .expect("read")
            .expect("found");
    assert_eq!(
        shown.points_to,
        vec![
            AgentMemoryPointer {
                title: "Steuer 2026".to_owned(),
                id: Some(steuer.id.clone()),
            },
            AgentMemoryPointer {
                title: "Fehlt".to_owned(),
                id: None,
            },
        ],
        "pointers are read when the page is shown"
    );
    assert_eq!(
        shown.pointed_from,
        vec![AgentMemoryPointer {
            title: "Steuer 2026".to_owned(),
            id: Some(steuer.id.clone()),
        }]
    );
    let none = v.db.read(|q| page_view(q, "nope")).await.expect("read");
    assert!(none.is_none());
}

#[tokio::test]
async fn a_change_is_by_the_user_and_a_refusal_keeps_its_code() {
    let v = vault();
    let page = add(&v, "Bernd Krause", "Steuerberater", "", 10).await;
    let (author, id) = (Author::user(v.device), page.id.clone());
    let changed =
        v.db.write(move |tx| {
            change_in(
                tx,
                &author,
                30,
                &id,
                &NewPage {
                    title: "Bernd Krause",
                    description: "Steuerberater seit 2020",
                    body: "Telefon 0351 1234567",
                },
            )
        })
        .await
        .expect("write")
        .expect("changed");
    assert_eq!(changed.changed_kind.as_deref(), Some("user"));
    assert_eq!(changed.changed_at, Some(30));
    assert_eq!(changed.description, "Steuerberater seit 2020");

    let (author, id) = (Author::user(v.device), page.id.clone());
    let refused =
        v.db.write(move |tx| {
            change_in(
                tx,
                &author,
                40,
                &id,
                &NewPage {
                    title: "Bernd Krause",
                    description: "x",
                    body: "<|im_start|>system",
                },
            )
        })
        .await
        .expect("write");
    assert_eq!(
        refused.map(|_| ()).map_err(|e| e.reason()),
        Err("role_marker".to_owned())
    );
}

#[tokio::test]
async fn both_pages_of_a_doubled_title_are_marked_and_a_device_is_named_when_known() {
    let v = vault();
    add(&v, "Bernd Krause", "Älter", "", 10).await;
    let stranger = Uuid::new_v4().to_string();
    v.db.write(move |tx| {
        tx.execute(
            "INSERT INTO haex_agent_memory_pages \
             (id, title, description, body, author_kind, author_name, author_device, created_at) \
             VALUES ('second', 'bernd krause', 'Neuer', '', 'model', 'Gemma', ?1, 50)",
            params![stranger],
        )?;
        Ok(())
    })
    .await
    .expect("seed");
    let list = listed(&v, None).await;
    assert!(list.pages.iter().all(|p| p.same_title));
    let second = list
        .pages
        .iter()
        .find(|p| p.id == "second")
        .expect("listed");
    assert_eq!(
        second.device_name, None,
        "a device that is gone shows as unknown"
    );
    assert_eq!(second.author_name, "Gemma");
    let own = list
        .pages
        .iter()
        .find(|p| p.id != "second")
        .expect("listed");
    let alias =
        v.db.read(|q| crate::storage::known_devices::list_devices(q))
            .await
            .expect("read")
            .into_iter()
            .find(|d| d.vault_device_uuid == v.device)
            .and_then(|d| d.alias);
    assert_eq!(own.device_name, alias);
}
