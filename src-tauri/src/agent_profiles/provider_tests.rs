//! Providers as members of profiles (spec 083 PR M): a provider's membership counts for every model
//! it has, a later one too, in addition to the model's own; leaving "Standard" works the same way.

use uuid::Uuid;

use super::resolve::{model_available, profile_of, rights_of};
use super::subjects::{list_subjects, profile_views, SubjectKind};
use super::test_support::vault;
use super::{store, Subject, STANDARD_PROFILE_ID};
use crate::mcp_server::grants::{OperateLevel, Rights};
use crate::storage::models::{upsert_model, IntegrityStatus, ModelRow, SourceKind};
use crate::storage::providers::{insert_provider, Provider, ProviderCapability, ProviderKind};

fn provider(id: Uuid, name: &str) -> Provider {
    Provider {
        id,
        kind: ProviderKind::ApiKey,
        adapter: Some("openai".to_owned()),
        name: name.to_owned(),
        base_url: None,
        credentials: Some(vec![1]),
        created_at: 1,
        capability: ProviderCapability::Chat,
    }
}

fn model(provider: Uuid, remote: &str) -> ModelRow {
    ModelRow {
        id: format!("{provider}:{remote}"),
        provider_id: provider,
        name: remote.to_owned(),
        context_window: None,
        fetched_at: Some(1),
        tokenizer_repo: None,
        hf_repo: None,
        hf_filename: None,
        hf_revision: None,
        hf_revision_ref: None,
        file_sha256: None,
        integrity_status: IntegrityStatus::Unknown,
        source_kind: SourceKind::Catalog,
        capabilities: None,
    }
}

fn writing() -> Rights {
    Rights {
        operate: [("chat.write".to_owned(), OperateLevel::Write)]
            .into_iter()
            .collect(),
        ..Rights::default()
    }
}

/// Two providers with a model each, and a profile "Arbeit" with rights that is active.
async fn scene() -> (super::test_support::Vault, Uuid, Uuid, Uuid) {
    let v = vault();
    let (work, private, profile) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    v.db.write(move |tx| {
        insert_provider(tx, &provider(work, "LiteLLM"))?;
        insert_provider(tx, &provider(private, "OpenRouter"))?;
        upsert_model(tx, &model(work, "gpt"))?;
        upsert_model(tx, &model(private, "gpt"))?;
        store::create_profile(tx, profile, "Arbeit")?;
        store::set_rights(tx, profile, &writing())?;
        store::set_active(tx, profile)
    })
    .await
    .expect("scene");
    (v, work, private, profile)
}

#[tokio::test]
async fn a_providers_membership_gives_its_models_the_rights_of_the_profile_and_not_the_other_providers(
) {
    let (v, work, private, profile) = scene().await;
    v.db.write(move |tx| store::add_member(tx, &Subject::Provider(work), profile))
        .await
        .expect("member");
    let of = |provider: Uuid| Subject::Model(format!("{provider}:gpt"));
    let (a, b) = (of(work), of(private));
    let (granted, plain) =
        v.db.read(move |q| Ok((rights_of(q, &a)?, rights_of(q, &b)?)))
            .await
            .expect("rights");
    assert_eq!(
        granted.operate.get("chat.write"),
        Some(&OperateLevel::Write)
    );
    assert_ne!(plain.operate.get("chat.write"), Some(&OperateLevel::Write));
    let (a, b) = (of(work), of(private));
    let names =
        v.db.read(move |q| Ok((profile_of(q, &a)?.1, profile_of(q, &b)?.1)))
            .await
            .expect("names");
    assert_eq!(names.0, "Arbeit");
    assert_eq!(names.1, super::store::STANDARD_NAME);
}

#[tokio::test]
async fn a_model_the_provider_gets_later_is_a_member_without_any_write() {
    let (v, work, _, profile) = scene().await;
    v.db.write(move |tx| store::add_member(tx, &Subject::Provider(work), profile))
        .await
        .expect("member");
    v.db.write(move |tx| upsert_model(tx, &model(work, "new-model")))
        .await
        .expect("a new model");
    let later = format!("{work}:new-model");
    let (available, rights) =
        v.db.read(move |q| {
            Ok((
                model_available(q, &later)?,
                rights_of(q, &Subject::Model(later.clone()))?,
            ))
        })
        .await
        .expect("read");
    assert!(
        available,
        "the chat offers it while the active profile has its provider"
    );
    assert_eq!(rights.operate.get("chat.write"), Some(&OperateLevel::Write));
}

#[tokio::test]
async fn the_chat_offers_the_models_of_the_active_profiles_providers_and_members_only() {
    let (v, work, private, profile) = scene().await;
    let single = format!("{private}:gpt");
    let s = single.clone();
    v.db.write(move |tx| {
        store::add_member(tx, &Subject::Provider(work), profile)?;
        Ok(())
    })
    .await
    .expect("member");
    let (w, p) = (format!("{work}:gpt"), single.clone());
    let (by_provider, other) =
        v.db.read(move |q| Ok((model_available(q, &w)?, model_available(q, &p)?)))
            .await
            .expect("read");
    assert!(by_provider && !other);
    // The model of the other provider joins on its own: both are offered, each by its own way.
    v.db.write(move |tx| store::add_member(tx, &Subject::Model(s), profile))
        .await
        .expect("model member");
    let p = single;
    assert!(v
        .db
        .read(move |q| model_available(q, &p))
        .await
        .expect("read"));
}

#[tokio::test]
async fn a_provider_that_left_standard_takes_its_models_with_it() {
    let (v, work, private, _) = scene().await;
    v.db.write(|tx| store::set_active(tx, STANDARD_PROFILE_ID))
        .await
        .expect("Standard");
    v.db.write(move |tx| store::set_in_standard(tx, &Subject::Provider(work), false))
        .await
        .expect("left");
    let (w, p) = (format!("{work}:gpt"), format!("{private}:gpt"));
    let (left, stays) =
        v.db.read(move |q| Ok((model_available(q, &w)?, model_available(q, &p)?)))
            .await
            .expect("read");
    assert!(!left && stays);
}

#[tokio::test]
async fn the_lists_show_a_provider_its_models_through_it_and_count_the_models() {
    let (v, work, _, profile) = scene().await;
    v.db.write(move |tx| store::add_member(tx, &Subject::Provider(work), profile))
        .await
        .expect("member");
    let subjects = v.db.read(|q| list_subjects(q)).await.expect("subjects");
    let provider_row = subjects
        .iter()
        .find(|s| s.subject == format!("provider:{work}"))
        .expect("the provider is listed");
    assert_eq!(provider_row.kind, SubjectKind::Provider);
    assert_eq!(provider_row.name, "LiteLLM");
    assert_eq!(provider_row.profiles.len(), 1);
    let model_row = subjects
        .iter()
        .find(|s| s.subject == format!("model:{work}:gpt"))
        .expect("its model");
    assert!(model_row.profiles.is_empty(), "no membership of its own");
    assert_eq!(model_row.inherited.len(), 1, "but one through the provider");
    assert_eq!(model_row.profile_name, "Arbeit");
    assert_eq!(
        model_row.provider_subject.as_deref(),
        Some(format!("provider:{work}").as_str())
    );
    let views = v.db.read(|q| profile_views(q)).await.expect("views");
    let arbeit = views.iter().find(|p| p.name == "Arbeit").expect("profile");
    assert_eq!(
        arbeit.member_count, 1,
        "the model counts, the provider does not"
    );
}

#[tokio::test]
async fn an_unreadable_provider_membership_fails_closed_even_when_standard_is_active() {
    let (v, work, _, _) = scene().await;
    let provider_subject = format!("provider:{work}");
    v.db.write(move |tx| {
        tx.execute(
            "INSERT INTO haex_agent_profile_members (id, subject, profile_id) VALUES (?1, ?2, ?3)",
            haex_crdt::rusqlite::params![
                "not-the-member-id",
                provider_subject,
                STANDARD_PROFILE_ID.to_string()
            ],
        )?;
        store::set_active(tx, STANDARD_PROFILE_ID)
    })
    .await
    .expect("malformed membership");

    let model = format!("{work}:gpt");
    let (rights, available) =
        v.db.read(move |q| {
            Ok((
                rights_of(q, &Subject::Model(model.clone()))?,
                model_available(q, &model)?,
            ))
        })
        .await
        .expect("read");
    assert_eq!(rights, Rights::default());
    assert!(
        !available,
        "a corrupt provider row must not offer the model"
    );

    let subjects = v.db.read(|q| list_subjects(q)).await.expect("subjects");
    let model = subjects
        .iter()
        .find(|subject| subject.subject == format!("model:{work}:gpt"))
        .expect("model");
    assert!(!model.available);
    assert!(!model.in_standard);
    assert!(model.profiles.is_empty());
}

#[tokio::test]
async fn provider_memberships_without_a_provider_or_models_remain_listed_and_removable() {
    let (v, _, _, profile) = scene().await;
    let missing_provider = Uuid::new_v4();
    v.db.write(move |tx| store::add_member(tx, &Subject::Provider(missing_provider), profile))
        .await
        .expect("membership for provider without a record");

    let subjects = v.db.read(|q| list_subjects(q)).await.expect("subjects");
    let provider = subjects
        .iter()
        .find(|subject| subject.subject == format!("provider:{missing_provider}"))
        .expect("provider membership stays visible without models");
    assert!(provider.missing);
    assert_eq!(provider.profiles.len(), 1);
    assert!(provider.name.contains(&missing_provider.to_string()));
}

#[test]
fn a_provider_subject_has_its_own_key() {
    let id = Uuid::new_v4();
    assert_eq!(Subject::Provider(id).key(), format!("provider:{id}"));
    assert_eq!(
        Subject::parse(&format!("provider:{id}")),
        Some(Subject::Provider(id))
    );
    assert_eq!(Subject::parse("provider:not-a-uuid"), None);
}
