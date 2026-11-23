//! Tests for the table of pages (spec 097 data-model.md, T012), against an opened test vault.

// The doubled title is seeded below the store, as a sync would bring it.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use uuid::Uuid;

use super::super::check::{CheckError, Field};
use super::super::limits::{LimitError, BODY_MAX_CHARS, MAX_PAGES};
use super::super::{Author, AuthorKind};
use super::*;
use crate::agent_profiles::test_support::{vault, Vault};

fn model(device: Uuid) -> Author {
    Author {
        kind: AuthorKind::Model,
        key: Some("model:p1:qwen".to_owned()),
        name: "Qwen3.5 4B".to_owned(),
        device,
    }
}

async fn add(v: &Vault, title: &str, description: &str, body: &str, now: i64) -> Page {
    let (author, title, description, body) = (
        Author::user(v.device),
        title.to_owned(),
        description.to_owned(),
        body.to_owned(),
    );
    v.db.write(move |tx| {
        settle(add_page(
            tx,
            &author,
            now,
            &NewPage {
                title: &title,
                description: &description,
                body: &body,
            },
        ))
    })
    .await
    .expect("write")
    .expect("added")
}

async fn try_write<T: Send + 'static>(
    v: &Vault,
    f: impl FnOnce(&mut haex_crdt::CrdtTransaction<'_>) -> Result<T, MemoryError> + Send + 'static,
) -> Result<T, MemoryError> {
    v.db.write(move |tx| settle(f(tx))).await.expect("write")
}

async fn pages(v: &Vault) -> Vec<Page> {
    v.db.read(|q| list(q)).await.expect("read")
}

#[tokio::test]
async fn a_page_keeps_its_author_and_a_change_its_changer() {
    let v = vault();
    let page = add(&v, "Bernd Krause", "Steuerberater", "Text", 10).await;
    assert_eq!(page.author_kind, "user");
    assert_eq!(page.author_key, None);
    assert_eq!(page.author_name, "");
    assert_eq!(page.author_device, v.device.to_string());
    assert_eq!(page.created_at, 10);
    assert_eq!(page.changed_at, None);

    let (id, writer) = (page.id.clone(), model(v.device));
    let changed = try_write(&v, move |tx| {
        change_page(
            tx,
            &writer,
            20,
            &id,
            &NewPage {
                title: "Bernd Krause",
                description: "Steuerberater seit 2020",
                body: "Neuer Text",
            },
        )
    })
    .await
    .expect("changed");
    assert_eq!(changed.author_kind, "user", "the author stays");
    assert_eq!(changed.changed_kind.as_deref(), Some("model"));
    assert_eq!(changed.changed_key.as_deref(), Some("model:p1:qwen"));
    assert_eq!(changed.changed_name.as_deref(), Some("Qwen3.5 4B"));
    assert_eq!(changed.changed_at, Some(20));
    assert_eq!(changed.body, "Neuer Text");
}

#[tokio::test]
async fn a_title_is_taken_without_case_and_spaces_but_a_page_keeps_its_own() {
    let v = vault();
    let first = add(&v, "Bernd Krause", "Steuerberater", "", 1).await;
    add(&v, "Mail-Stil", "Wie ich Mails schreibe", "", 2).await;
    let author = Author::user(v.device);
    let again = try_write(&v, move |tx| {
        add_page(
            tx,
            &author,
            3,
            &NewPage {
                title: "  bernd KRAUSE ",
                description: "Doppelt",
                body: "",
            },
        )
    })
    .await;
    assert!(matches!(again, Err(MemoryError::TitleTaken)), "{again:?}");

    let (author, id) = (Author::user(v.device), first.id.clone());
    let renamed = try_write(&v, move |tx| {
        change_page(
            tx,
            &author,
            4,
            &id,
            &NewPage {
                title: "mail-stil",
                description: "x",
                body: "",
            },
        )
    })
    .await;
    assert!(
        matches!(renamed, Err(MemoryError::TitleTaken)),
        "{renamed:?}"
    );

    let (author, id) = (Author::user(v.device), first.id.clone());
    let same = try_write(&v, move |tx| {
        change_page(
            tx,
            &author,
            5,
            &id,
            &NewPage {
                title: "BERND krause",
                description: "Steuerberater",
                body: "",
            },
        )
    })
    .await;
    assert!(
        same.is_ok(),
        "a page may keep or recase its own title: {same:?}"
    );
}

#[tokio::test]
async fn the_list_is_changed_last_first() {
    let v = vault();
    let a = add(&v, "A", "a", "", 10).await;
    let b = add(&v, "B", "b", "", 20).await;
    add(&v, "C", "c", "", 15).await;
    let (author, id) = (Author::user(v.device), a.id.clone());
    try_write(&v, move |tx| {
        change_page(
            tx,
            &author,
            30,
            &id,
            &NewPage {
                title: "A",
                description: "a",
                body: "neu",
            },
        )
    })
    .await
    .expect("changed");
    let titles: Vec<String> = pages(&v).await.into_iter().map(|p| p.title).collect();
    assert_eq!(titles, vec!["A", "B", "C"]);
    assert_eq!(b.last_change(), 20);
}

#[tokio::test]
async fn a_doubled_title_resolves_to_the_page_changed_last_and_names_the_other() {
    let v = vault();
    let older = add(&v, "Bernd Krause", "Älter", "", 10).await;
    let device = v.device.to_string();
    v.db.write(move |tx| {
        tx.execute(
            "INSERT INTO haex_agent_memory_pages \
             (id, title, description, body, author_kind, author_name, author_device, created_at) \
             VALUES (?1, 'bernd krause', 'Neuer', '', 'user', '', ?2, 50)",
            params![Uuid::new_v4().to_string(), device],
        )?;
        Ok(())
    })
    .await
    .expect("seed");
    let found =
        v.db.read(|q| by_title(q, "Bernd Krause"))
            .await
            .expect("read")
            .expect("found");
    assert_eq!(found.page.description, "Neuer");
    assert_eq!(
        found.also.as_ref().map(|p| p.id.as_str()),
        Some(older.id.as_str())
    );
    let missing = v.db.read(|q| by_title(q, "Niemand")).await.expect("read");
    assert!(missing.is_none());
}

#[tokio::test]
async fn every_write_is_checked() {
    let v = vault();
    let author = Author::user(v.device);
    let secret = try_write(&v, move |tx| {
        add_page(
            tx,
            &author,
            1,
            &NewPage {
                title: "WLAN",
                description: "Zuhause",
                body: "Mein WLAN-Passwort ist hunter2hunter2",
            },
        )
    })
    .await;
    assert!(
        matches!(
            secret,
            Err(MemoryError::Check(Field::Body, CheckError::Secret))
        ),
        "{secret:?}"
    );
    let author = Author::user(v.device);
    let long = try_write(&v, move |tx| {
        add_page(
            tx,
            &author,
            1,
            &NewPage {
                title: "Lang",
                description: "Zu lang",
                body: &"a".repeat(BODY_MAX_CHARS + 1),
            },
        )
    })
    .await;
    assert!(
        matches!(
            long,
            Err(MemoryError::Limit(LimitError::TooLong(Field::Body)))
        ),
        "{long:?}"
    );
    let author = Author::user(v.device);
    let empty = try_write(&v, move |tx| {
        add_page(
            tx,
            &author,
            1,
            &NewPage {
                title: "Leer",
                description: " ",
                body: "",
            },
        )
    })
    .await;
    assert!(
        matches!(
            empty,
            Err(MemoryError::Check(Field::Description, CheckError::Empty))
        ),
        "{empty:?}"
    );
    assert!(pages(&v).await.is_empty(), "nothing refused was stored");
}

#[tokio::test]
async fn an_addition_appends_a_paragraph_or_creates_the_page() {
    let v = vault();
    let author = model(v.device);
    let created = try_write(&v, move |tx| {
        append(tx, &author, 1, "Mail-Stil", "Kurz und ohne Anrede.", None)
    })
    .await;
    assert!(
        matches!(created, Err(MemoryError::DescriptionNeeded)),
        "{created:?}"
    );

    let author = model(v.device);
    let (page, was_new) = try_write(&v, move |tx| {
        append(
            tx,
            &author,
            2,
            "Mail-Stil",
            "Kurz und ohne Anrede.",
            Some("Wie ich Mails beantworte"),
        )
    })
    .await
    .expect("created");
    assert!(was_new);
    assert_eq!(page.author_name, "Qwen3.5 4B");

    let author = model(v.device);
    let (page, was_new) = try_write(&v, move |tx| {
        append(
            tx,
            &author,
            3,
            "mail-stil",
            "Grußformel: Viele Grüße.",
            None,
        )
    })
    .await
    .expect("appended");
    assert!(!was_new);
    assert_eq!(
        page.body,
        "Kurz und ohne Anrede.\n\nGrußformel: Viele Grüße."
    );
    assert_eq!(page.changed_at, Some(3));

    let author = model(v.device);
    let over = try_write(&v, move |tx| {
        append(
            tx,
            &author,
            4,
            "Mail-Stil",
            &"x".repeat(BODY_MAX_CHARS),
            None,
        )
    })
    .await;
    assert!(
        matches!(over, Err(MemoryError::Limit(LimitError::PageTooLong))),
        "{over:?}"
    );
}

#[tokio::test]
async fn a_full_memory_takes_no_new_page_but_can_be_tidied() {
    let v = vault();
    let device = v.device.to_string();
    v.db.write(move |tx| {
        for n in 0..=MAX_PAGES {
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
    .expect("seed a memory over its limit, as a sync can leave it");
    let author = Author::user(v.device);
    let full = try_write(&v, move |tx| {
        add_page(
            tx,
            &author,
            1,
            &NewPage {
                title: "Neu",
                description: "d",
                body: "",
            },
        )
    })
    .await;
    assert!(
        matches!(full, Err(MemoryError::Limit(LimitError::MemoryFull))),
        "{full:?}"
    );

    let first = pages(&v).await.remove(0);
    let (author, id) = (Author::user(v.device), first.id.clone());
    try_write(&v, move |tx| {
        change_page(
            tx,
            &author,
            2,
            &id,
            &NewPage {
                title: &first.title,
                description: "d",
                body: "Tex",
            },
        )
    })
    .await
    .expect("a shrinking change runs over the limit");
    let id = pages(&v).await.remove(0).id;
    let removed = try_write(&v, move |tx| remove_page(tx, &id)).await;
    assert!(removed.is_ok(), "a removal runs over the limit");
    assert_eq!(pages(&v).await.len(), MAX_PAGES);
}

#[tokio::test]
async fn an_unknown_page_is_not_found() {
    let v = vault();
    let author = Author::user(v.device);
    let changed = try_write(&v, move |tx| {
        change_page(
            tx,
            &author,
            1,
            "nope",
            &NewPage {
                title: "T",
                description: "d",
                body: "",
            },
        )
    })
    .await;
    assert!(matches!(changed, Err(MemoryError::NotFound)), "{changed:?}");
    let removed = try_write(&v, |tx| remove_page(tx, "nope")).await;
    assert!(matches!(removed, Err(MemoryError::NotFound)), "{removed:?}");
}

#[test]
fn the_reasons_name_the_code_and_the_field() {
    assert_eq!(MemoryError::TitleTaken.reason(), "title_taken");
    assert_eq!(
        MemoryError::Limit(LimitError::TooLong(Field::Title)).reason(),
        "too_long:title"
    );
    assert_eq!(
        MemoryError::Check(Field::Description, CheckError::LineBreak).reason(),
        "line_break:description"
    );
    assert_eq!(
        MemoryError::Check(Field::Body, CheckError::Secret).reason(),
        "secret"
    );
    assert_eq!(
        MemoryError::Limit(LimitError::MemoryFull).reason(),
        "memory_full"
    );
}
