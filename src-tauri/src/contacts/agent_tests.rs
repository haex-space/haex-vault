//! The built-in agent's way into the address book (spec 051 US6, T079, T080): its grants from the
//! rows of `agent_file_permissions`, the question when none covers a call, what is kept of an
//! answer, and that a refusal looks like a missing contact.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::*;
use crate::contacts::model::{ContactInput, Head, ValueInput, ValueUpsert, ValuesPatch};
use crate::contacts::phone::test_rig::Rig;
use crate::contacts::phone::test_support::{name, phone, GOOGLE};
use crate::contacts::phone::Mode;
use crate::contacts::service::test_support::{test_vault, TestVault};
use crate::files::agent::prompt::{
    FilesAgentAnswer, FilesAgentPermissionRequest, FilesAgentQuestionKind,
};

fn rows(list: &[(&str, AgentFileStatus)]) -> Vec<(String, AgentFileStatus)> {
    list.iter()
        .map(|(target, status)| ((*target).to_owned(), *status))
        .collect()
}

use AgentFileStatus::{Denied, Read, ReadWrite};

#[test]
fn rows_become_grants_and_a_denied_star_takes_them_all() {
    assert!(grants_from(&[]).is_empty());
    assert_eq!(
        grants_from(&rows(&[("*", Read)])),
        [Grant::new(GrantAction::Read, Scope::All)]
    );
    assert_eq!(
        grants_from(&rows(&[("group:g1", ReadWrite)])),
        [Grant::new(
            GrantAction::ReadWrite,
            Scope::Groups(["g1".to_owned()].into())
        )]
    );
    assert!(grants_from(&rows(&[("*", Denied), ("group:g1", ReadWrite)])).is_empty());
    assert_eq!(
        grants_from(&rows(&[("*", Read), ("group:g2", Denied)])),
        [Grant::new(
            GrantAction::Read,
            Scope::all_except(["g2".to_owned()].into())
        )]
    );
}

#[test]
fn holzi_asks_only_when_no_row_covers_the_call_and_never_under_a_denied_star() {
    assert!(asks(&[], GrantAction::Read));
    assert!(!asks(&rows(&[("*", Read)]), GrantAction::Read));
    assert!(asks(&rows(&[("*", Read)]), GrantAction::ReadWrite));
    assert!(!asks(&rows(&[("*", ReadWrite)]), GrantAction::ReadWrite));
    assert!(!asks(&rows(&[("*", Denied)]), GrantAction::Read));
    assert!(!asks(&rows(&[("group:g1", Read)]), GrantAction::Read));
}

#[test]
fn a_contacts_row_needs_star_or_a_group_target() {
    let row = |target: &str| AgentFilePermission {
        agent_id: "builtin".into(),
        kind: AgentFileKind::Contacts,
        target: target.into(),
        status: Read,
        updated_at: 1,
        reach: None,
    };
    let vault = test_vault();
    for (target, ok) in [
        ("*", true),
        ("group:g1", true),
        ("group:", false),
        ("", false),
    ] {
        let row = row(target);
        let result = vault
            .db
            .write_blocking(move |tx| Ok(permissions::set(tx, &row)?));
        assert_eq!(result.is_ok(), ok, "{target}");
    }
    let stored = vault
        .db
        .read_blocking(|q| Ok(permissions::contacts_of(q, "builtin")?))
        .unwrap();
    assert_eq!(stored.len(), 2);
}

/// A prompt whose window records each question and answers it with `answer`; `None` is no window.
fn window(
    answer: Option<FilesAgentAnswer>,
) -> (
    PermissionPrompt,
    Arc<Mutex<Vec<FilesAgentPermissionRequest>>>,
) {
    let prompt = PermissionPrompt::with_timeout(Duration::from_secs(5));
    let asked = Arc::new(Mutex::new(Vec::new()));
    if let Some(answer) = answer {
        let (seen, answering) = (asked.clone(), prompt.clone());
        prompt.set_emitter(Arc::new(move |request| {
            seen.lock().unwrap().push(request.clone());
            let (answering, id) = (answering.clone(), request.request_id.clone());
            tokio::spawn(async move {
                answering.answer(&id, answer);
            });
            true
        }));
    }
    (prompt, asked)
}

fn once(choice: FilesAgentChoice) -> Option<FilesAgentAnswer> {
    Some(FilesAgentAnswer::plain(choice))
}

fn always(choice: FilesAgentChoice) -> Option<FilesAgentAnswer> {
    Some(FilesAgentAnswer {
        choice,
        remember: true,
        also_cloud: false,
    })
}

fn person(given: &str, groups: Vec<String>) -> ContactInput {
    ContactInput {
        head: Head {
            given_name: Some(given.to_owned()),
            ..Head::default()
        },
        values: vec![ValueInput {
            kind: "phone".to_owned(),
            label: "mobile".to_owned(),
            value: "0170 1111".to_owned(),
        }],
        groups,
        targets: vec![],
    }
}

async fn with_ben(vault: &TestVault) -> String {
    vault
        .service
        .create(&Caller::User, &[], person("Ben", vec![]))
        .await
        .unwrap()
        .id
}

fn stored(vault: &TestVault) -> Vec<(String, AgentFileStatus)> {
    vault
        .db
        .read_blocking(|q| Ok(permissions::contacts_of(q, "builtin")?))
        .unwrap()
}

fn agent(vault: &TestVault, prompt: PermissionPrompt) -> ContactsAgent {
    ContactsAgent::builtin(vault.db.clone(), prompt)
}

fn renumber(value_id: &str) -> ContactPatch {
    ContactPatch {
        values: Some(ValuesPatch {
            upsert: vec![ValueUpsert {
                id: Some(value_id.to_owned()),
                kind: "phone".to_owned(),
                label: "mobile".to_owned(),
                value: "0170 2222".to_owned(),
                position: None,
            }],
            remove: vec![],
        }),
        ..ContactPatch::default()
    }
}

#[tokio::test]
async fn without_a_grant_holzi_asks_and_a_denial_is_kept_and_looks_like_nothing() {
    let vault = test_vault();
    let ben = with_ben(&vault).await;
    let (prompt, asked) = window(once(FilesAgentChoice::Deny));
    let agent = agent(&vault, prompt);
    let cancel = CancellationToken::new();

    let refused = agent.search("Ben", None, &cancel).await.unwrap_err();
    assert!(matches!(refused, HolziError::ContactsNotFound));
    let missing = agent.get("no-such-contact", &cancel).await.unwrap_err();
    let hidden = agent.get(&ben, &cancel).await.unwrap_err();
    assert_eq!(missing.to_string(), hidden.to_string(), "FR-031");
    let questions = asked.lock().unwrap().clone();
    assert_eq!(questions.len(), 1, "a kept denial asks no more");
    assert_eq!(questions[0].kind, FilesAgentQuestionKind::Contacts);
    assert_eq!(questions[0].wants, FilesAgentWant::Read);
    assert_eq!(stored(&vault), rows(&[("*", Denied)]));
}

#[tokio::test]
async fn nobody_to_ask_refuses_without_keeping_anything() {
    let vault = test_vault();
    with_ben(&vault).await;
    let (prompt, _) = window(None);
    let refused = agent(&vault, prompt)
        .search("Ben", None, &CancellationToken::new())
        .await;
    assert!(matches!(refused, Err(HolziError::ContactsNotFound)));
    assert!(stored(&vault).is_empty());
}

#[tokio::test]
async fn allowed_once_holds_for_the_call_and_asks_again() {
    let vault = test_vault();
    with_ben(&vault).await;
    let (prompt, asked) = window(once(FilesAgentChoice::Read));
    let agent = agent(&vault, prompt);
    let cancel = CancellationToken::new();
    for _ in 0..2 {
        let found = agent.search("ben", None, &cancel).await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].display_name, "Ben");
    }
    assert_eq!(asked.lock().unwrap().len(), 2);
    assert!(stored(&vault).is_empty());
}

#[tokio::test]
async fn a_read_grant_reads_but_never_changes_and_a_denied_write_keeps_the_read() {
    let vault = test_vault();
    let ben = with_ben(&vault).await;
    let (prompt, asked) = window(always(FilesAgentChoice::Read));
    let agent_read = agent(&vault, prompt);
    let cancel = CancellationToken::new();
    let contact = agent_read.get(&ben, &cancel).await.unwrap();
    assert_eq!(stored(&vault), rows(&[("*", Read)]));
    assert_eq!(asked.lock().unwrap().len(), 1);

    let (prompt, asked) = window(once(FilesAgentChoice::Deny));
    let agent_write = agent(&vault, prompt);
    let refused = agent_write
        .update(&ben, renumber(&contact.values[0].id), &cancel)
        .await;
    assert!(matches!(refused, Err(HolziError::ContactsNotFound)));
    assert_eq!(asked.lock().unwrap()[0].wants, FilesAgentWant::ReadWrite);
    assert_eq!(stored(&vault), rows(&[("*", Read)]), "the read stays");
    let unchanged = agent_write.get(&ben, &cancel).await.unwrap();
    assert_eq!(unchanged.values[0].value, "0170 1111");
}

#[tokio::test]
async fn a_group_read_grant_survives_a_denied_write() {
    let vault = test_vault();
    let work = vault
        .service
        .group_save(&Caller::User, None, "Arbeit".into())
        .await
        .unwrap();
    let carla = vault
        .service
        .create(&Caller::User, &[], person("Carla", vec![work.id.clone()]))
        .await
        .unwrap();
    let row = AgentFilePermission {
        agent_id: "builtin".into(),
        kind: AgentFileKind::Contacts,
        target: permissions::group_target(&work.id),
        status: Read,
        updated_at: 1,
        reach: None,
    };
    vault
        .db
        .write_blocking(move |tx| Ok(permissions::set(tx, &row)?))
        .unwrap();

    let (prompt, asked) = window(once(FilesAgentChoice::Deny));
    let agent = agent(&vault, prompt);
    let refused = agent
        .update(
            &carla.id,
            renumber(&carla.values[0].id),
            &CancellationToken::new(),
        )
        .await;
    assert!(matches!(refused, Err(HolziError::ContactsNotFound)));
    assert_eq!(asked.lock().unwrap().len(), 1);
    let target = permissions::group_target(&work.id);
    assert_eq!(stored(&vault), rows(&[(&target, Read)]));
    let readable = agent.get(&carla.id, &CancellationToken::new()).await;
    assert!(readable.is_ok(), "the group read grant must survive");
}

#[tokio::test]
async fn a_group_grant_shows_only_its_members_without_asking() {
    let vault = test_vault();
    let work = vault
        .service
        .group_save(&Caller::User, None, "Arbeit".into())
        .await
        .unwrap();
    vault
        .service
        .create(&Caller::User, &[], person("Carla", vec![work.id.clone()]))
        .await
        .unwrap();
    with_ben(&vault).await;
    let row = AgentFilePermission {
        agent_id: "builtin".into(),
        kind: AgentFileKind::Contacts,
        target: permissions::group_target(&work.id),
        status: Read,
        updated_at: 1,
        reach: None,
    };
    vault
        .db
        .write_blocking(move |tx| Ok(permissions::set(tx, &row)?))
        .unwrap();
    let (prompt, asked) = window(once(FilesAgentChoice::Deny));
    let found = agent(&vault, prompt)
        .search("", None, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(
        found
            .iter()
            .map(|c| c.display_name.as_str())
            .collect::<Vec<_>>(),
        ["Carla"]
    );
    assert_eq!(found[0].groups, ["Arbeit"]);
    assert!(asked.lock().unwrap().is_empty());
}

#[tokio::test]
async fn agents_never_change_groups() {
    let vault = test_vault();
    let ben = with_ben(&vault).await;
    let (prompt, _) = window(always(FilesAgentChoice::ReadWrite));
    let patch = ContactPatch {
        groups: Some(vec![]),
        ..ContactPatch::default()
    };
    let refused = agent(&vault, prompt)
        .update(&ben, patch, &CancellationToken::new())
        .await;
    assert!(matches!(refused, Err(HolziError::ContactsInvalid { .. })));
}

#[tokio::test]
async fn a_change_with_a_write_grant_reaches_the_phone_like_a_change_in_the_window() {
    let rig = Rig::new();
    let raw = rig
        .book
        .add(GOOGLE, vec![name("Anna", "Beck"), phone("0170 1111", 2)]);
    rig.start();
    let anna = rig.named("Anna Beck");
    let (prompt, _) = window(always(FilesAgentChoice::ReadWrite));
    let changed = ContactsAgent::builtin(rig.vault.db.clone(), prompt)
        .update(
            &anna.id,
            renumber(&anna.values[0].id),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(changed.values[0].value, "0170 2222");
    let summary = rig.run(Mode::Normal).expect("run");
    assert_eq!(summary.counts.written, 1);
    assert_eq!(rig.book.values(raw, "Phone"), ["0170 2222"]);
    assert_eq!(
        rig.contact(&anna.id).origin.kind.as_str(),
        "phone",
        "origin unchanged"
    );
}

#[tokio::test]
async fn an_agent_with_a_read_grant_sees_the_own_contact_with_its_real_name_and_is_self() {
    let vault = test_vault();
    vault
        .service
        .update(
            &Caller::User,
            &[],
            crate::contacts::ids::SELF_CONTACT_ID,
            ContactPatch {
                given_name: crate::patch::Patch::Set("Martin".to_owned()),
                family_name: crate::patch::Patch::Set("Drechsel".to_owned()),
                ..ContactPatch::default()
            },
        )
        .await
        .unwrap();
    vault
        .db
        .write_blocking(|tx| {
            Ok(permissions::set(
                tx,
                &AgentFilePermission {
                    agent_id: "builtin".into(),
                    kind: AgentFileKind::Contacts,
                    target: "*".into(),
                    status: Read,
                    updated_at: 1,
                    reach: None,
                },
            )?)
        })
        .unwrap();
    let (prompt, asked) = window(once(FilesAgentChoice::Deny));
    let agent = agent(&vault, prompt);
    let found = agent
        .search("Drechsel", None, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].display_name, "Martin Drechsel", "not the word Ich");
    assert!(found[0].is_self);
    let json = serde_json::to_value(&found[0]).unwrap();
    assert_eq!(json["isSelf"], true);

    let own = agent
        .get(
            crate::contacts::ids::SELF_CONTACT_ID,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(own.display_name, "Martin Drechsel");
    assert!(own.is_self);
    assert!(asked.lock().unwrap().is_empty());
}

#[tokio::test]
async fn other_contacts_carry_no_is_self() {
    let vault = test_vault();
    let ben = with_ben(&vault).await;
    vault
        .db
        .write_blocking(|tx| {
            Ok(permissions::set(
                tx,
                &AgentFilePermission {
                    agent_id: "builtin".into(),
                    kind: AgentFileKind::Contacts,
                    target: "*".into(),
                    status: Read,
                    updated_at: 1,
                    reach: None,
                },
            )?)
        })
        .unwrap();
    let (prompt, _) = window(once(FilesAgentChoice::Deny));
    let found = agent(&vault, prompt)
        .get(&ben, &CancellationToken::new())
        .await
        .unwrap();
    assert!(!found.is_self);
    assert!(serde_json::to_value(&found)
        .unwrap()
        .get("isSelf")
        .is_none());
}
