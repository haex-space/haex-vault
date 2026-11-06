//! Tests for the subjects the pages list (spec 083 FR-008, FR-016, T055): the local agent's
//! models, the member counts, and a model that is gone keeping its assignment.

use uuid::Uuid;

use super::subjects::{list_subjects, profile_views, SubjectKind};
use super::test_support::vault;
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
        store::assign(tx, &Subject::Model("installed-model".to_owned()), profile)?;
        store::assign(tx, &Subject::Model("gone-model".to_owned()), profile)?;
        // A local model is listed by its row, whatever the state of its file on this device.
        upsert_model(tx, &row(provider, "catalog-unused", "Catalog unused", None))?;
        store::assign(tx, &Subject::Model("catalog-only".to_owned()), profile)
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
    // Only the assigned ones are in the profile; the others are in "Standard".
    let in_profile: Vec<&str> = models
        .iter()
        .filter(|subject| subject.profile_id == profile.to_string())
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
