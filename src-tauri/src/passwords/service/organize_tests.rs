//! Tests for the tag methods of the service that spec 100 adds: only the user merges tags, a merge
//! through the service moves the entries, and the profiles that name a tag are found by its id.

use std::sync::Arc;

use super::*;
use crate::passwords::ids::tag_id;
use crate::passwords::items;
use crate::passwords::model::ItemInput;
use crate::passwords::test_support::open_test_vault;
use crate::vault_gate::VaultGate;

struct Setup {
    _dir: tempfile::TempDir,
    service: PasswordsService,
    vault: crate::vault_gate::VaultDb,
    entry: String,
    source: String,
    target: String,
}

/// A vault with one entry that carries the tags "Job" and "Arbeit".
fn setup() -> Setup {
    let (dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db)).unwrap();
    let entry = vault
        .write_blocking(|tx| {
            let input = ItemInput {
                tags: vec!["Job".to_owned(), "Arbeit".to_owned()],
                ..ItemInput::default()
            };
            Ok(items::create_item(tx, &input, None)?)
        })
        .unwrap();
    Setup {
        _dir: dir,
        service: PasswordsService::new(vault.clone()),
        vault,
        entry,
        source: tag_id("Job").to_string(),
        target: tag_id("Arbeit").to_string(),
    }
}

fn names(s: &Setup) -> Vec<String> {
    let entry = s.entry.clone();
    s.vault
        .read_blocking(move |q| Ok(tags::names_of_item(q, &entry)?))
        .unwrap()
}

#[tokio::test]
async fn the_user_merges_a_tag_into_another() {
    let s = setup();
    s.service
        .merge_tags(
            &Caller::User,
            vec![s.source.clone()],
            vec![s.target.clone()],
            None,
        )
        .await
        .unwrap();
    assert_eq!(names(&s), ["Arbeit"]);
}

#[tokio::test]
async fn nobody_but_the_user_merges_tags_and_nothing_changes() {
    let s = setup();
    let refused = s
        .service
        .merge_tags(
            &Caller::BuiltinAgent,
            vec![s.source.clone()],
            vec![s.target.clone()],
            Some("Beruf".to_owned()),
        )
        .await;
    assert!(matches!(refused, Err(HaexError::PasswordsForbidden)));
    assert_eq!(names(&s), ["Arbeit", "Job"]);
}

#[tokio::test]
async fn the_profiles_of_a_tag_are_asked_by_its_id() {
    let s = setup();
    assert!(s
        .service
        .tag_agent_profiles(&Caller::User, s.source.clone())
        .await
        .unwrap()
        .is_empty());
    let missing = s
        .service
        .tag_agent_profiles(&Caller::User, "nope".to_owned())
        .await;
    assert!(matches!(missing, Err(HaexError::PasswordsNotFound)));
}

#[tokio::test]
async fn only_the_user_can_look_up_tag_agent_profiles() {
    let s = setup();
    let refused = s
        .service
        .tag_agent_profiles(&Caller::BuiltinAgent, s.source.clone())
        .await;
    assert!(matches!(refused, Err(HaexError::PasswordsForbidden)));
}

#[tokio::test]
async fn the_user_names_the_merged_tag() {
    let s = setup();
    s.service
        .merge_tags(
            &Caller::User,
            vec![s.source.clone()],
            vec![s.target.clone()],
            Some("Beruf".to_owned()),
        )
        .await
        .unwrap();
    assert_eq!(names(&s), ["Beruf"]);
}
