//! Tests for the subjects the pages list (spec 083 FR-008, FR-016, T055): the local agent's
//! models, the member counts, and a model that is gone keeping its assignment.

use uuid::Uuid;

use super::subjects::{list_subjects, profile_views, SubjectKind};
use super::test_support::vault;
use super::STANDARD_PROFILE_ID;
use super::{store, Subject};
use crate::providers::local::ensure_local_provider;
use crate::storage::models::{upsert_model, IntegrityStatus, ModelRow, SourceKind};

fn row(provider: Uuid, id: &str, name: &str, sha: Option<&str>) -> ModelRow {
    ModelRow {
        id: id.to_owned(),
        provider_id: provider,
        name: name.to_owned(),
        context_window: None,
        fetched_at: None,
        tokenizer_repo: None,
        hf_repo: None,
        hf_filename: None,
        hf_revision: None,
        hf_revision_ref: None,
        file_sha256: sha.map(str::to_owned),
        integrity_status: IntegrityStatus::Unknown,
        source_kind: SourceKind::Catalog,
        capabilities: None,
    }
}

#[tokio::test]
async fn every_local_model_is_listed_and_an_assigned_model_that_is_gone_is_missing() {
    let v = vault();
    let profile = Uuid::new_v4();
    v.db.write(move |tx| {
        let provider = ensure_local_provider(tx)?;
        upsert_model(
            tx,
            &row(provider, "installed-model", "Installed", Some("abc")),
        )?;
        upsert_model(tx, &row(provider, "catalog-only", "Catalog only", None))?;
        store::create_profile(tx, profile, "Careful")?;
        store::add_member(tx, &Subject::Model("installed-model".to_owned()), profile)?;
        store::add_member(tx, &Subject::Model("gone-model".to_owned()), profile)?;
        // A local model is listed by its row, whatever the state of its file on this device.
        upsert_model(tx, &row(provider, "catalog-unused", "Catalog unused", None))?;
        store::add_member(tx, &Subject::Model("catalog-only".to_owned()), profile)
    })
    .await
    .expect("setup");

    let subjects = v.db.read(|q| list_subjects(q)).await.expect("subjects");
    let models: Vec<_> = subjects
        .iter()
        .filter(|subject| subject.kind == SubjectKind::Builtin)
        .collect();
    let names: Vec<(&str, bool)> = models
        .iter()
        .map(|subject| (subject.name.as_str(), subject.missing))
        .collect();
    assert_eq!(
        names,
        [
            ("Catalog only", false),
            ("Catalog unused", false),
            ("gone-model", true),
            ("Installed", false)
        ]
    );
    // Only the added ones are members of the profile; it is not active, so every model has the
    // rights of "Standard".
    let in_profile: Vec<&str> = models
        .iter()
        .filter(|subject| subject.profiles.iter().any(|p| p.id == profile.to_string()))
        .map(|subject| subject.name.as_str())
        .collect();
    assert_eq!(in_profile, ["Catalog only", "gone-model", "Installed"]);
    // A model that is gone is no local model; the others are.
    let local: Vec<&str> = models
        .iter()
        .filter(|subject| subject.local)
        .map(|subject| subject.name.as_str())
        .collect();
    assert_eq!(local, ["Catalog only", "Catalog unused", "Installed"]);

    // The assignment of the gone model stays, and it counts as a member of its profile.
    let views = v.db.read(|q| profile_views(q)).await.expect("views");
    let careful = views
        .iter()
        .find(|view| view.id == profile.to_string())
        .expect("profile");
    assert_eq!(careful.member_count, 3);
}

#[tokio::test]
async fn the_active_profile_applies_to_its_members_and_standard_counts_every_subject() {
    let v = vault();
    let (profile, other, agent) = (
        Uuid::new_v4(),
        Uuid::new_v4(),
        super::test_support::registered(&v, "codex").await,
    );
    v.db.write(move |tx| {
        let provider = ensure_local_provider(tx)?;
        upsert_model(tx, &row(provider, "member", "Member", Some("abc")))?;
        upsert_model(tx, &row(provider, "other", "Other", Some("def")))?;
        store::create_profile(tx, profile, "Sensibel")?;
        store::create_profile(tx, other, "Offen")?;
        // The same model is a member of two profiles; the agent of none.
        store::add_member(tx, &Subject::Model("member".to_owned()), profile)?;
        store::add_member(tx, &Subject::Model("member".to_owned()), other)?;
        store::set_active(tx, profile)
    })
    .await
    .expect("setup");

    let subjects = v.db.read(|q| list_subjects(q)).await.expect("subjects");
    let member = subjects
        .iter()
        .find(|subject| subject.name == "Member")
        .expect("member");
    assert_eq!(
        member.profile_id,
        profile.to_string(),
        "the active one applies"
    );
    assert_eq!(
        member
            .profiles
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        ["Offen", "Sensibel"]
    );
    let others: Vec<_> = subjects
        .iter()
        .filter(|subject| subject.name != "Member")
        .collect();
    assert!(others.len() >= 2, "the other model and the agent {agent}");
    assert!(others.iter().all(
        |subject| subject.profile_id == STANDARD_PROFILE_ID.to_string()
            && subject.profiles.is_empty()
    ));

    let views = v.db.read(|q| profile_views(q)).await.expect("views");
    let count = |id: Uuid| {
        views
            .iter()
            .find(|view| view.id == id.to_string())
            .map(|view| (view.member_count, view.is_active))
    };
    assert_eq!(count(profile), Some((1, true)));
    assert_eq!(count(other), Some((1, false)));
    assert_eq!(
        count(STANDARD_PROFILE_ID),
        Some((u32::try_from(subjects.len()).expect("small"), false)),
        "Standard holds every subject"
    );
}
