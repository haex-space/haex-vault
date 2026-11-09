//! Tests for the update of the Abo-Agents (spec 083 PR J, FR-027, FR-031): a delegate's memberships
//! become those of its models, its models are listed like any model, and its row is no agent.

use uuid::Uuid;

use super::delegates::convert_memberships;
use super::subjects::{list_subjects, SubjectKind};
use super::test_support::vault;
use super::{store, Subject};
use crate::mcp_server::delegates::delegate_id;
use crate::mcp_server::registry::{self, AgentKind, AgentMode};
use crate::storage::models::{upsert_model, IntegrityStatus, ModelRow, SourceKind};
use crate::storage::providers::{insert_provider, Provider, ProviderCapability, ProviderKind};

fn provider(id: Uuid, adapter: &str) -> Provider {
    Provider {
        id,
        kind: ProviderKind::CliDelegate,
        adapter: Some(adapter.to_owned()),
        name: "Claude Code".to_owned(),
        base_url: None,
        credentials: Some(vec![1]),
        created_at: 1,
        capability: ProviderCapability::Chat,
    }
}

fn model(provider: Uuid, remote: &str, name: &str) -> ModelRow {
    ModelRow {
        id: format!("{provider}:{remote}"),
        provider_id: provider,
        name: name.to_owned(),
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

#[tokio::test]
async fn the_models_of_a_delegates_provider_are_listed_and_its_row_is_not() {
    let v = vault();
    let (device, provider_id) = (v.device, Uuid::new_v4());
    v.db.write(move |tx| {
        insert_provider(tx, &provider(provider_id, "claude"))?;
        upsert_model(tx, &model(provider_id, "sonnet", "Sonnet"))?;
        registry::register_delegate(
            tx,
            device,
            AgentKind::ClaudeCodeDelegate,
            "Claude Code",
            super::STANDARD_PROFILE_ID,
            AgentMode::Manual,
            1,
        )?;
        Ok(())
    })
    .await
    .expect("setup");
    let subjects = v.db.read(|q| list_subjects(q)).await.expect("subjects");
    assert!(
        subjects
            .iter()
            .all(|subject| subject.kind != SubjectKind::External),
        "the technical row of a delegate is no agent"
    );
    let sonnet = subjects
        .iter()
        .find(|subject| subject.name.starts_with("Sonnet"))
        .expect("its model is listed");
    assert_eq!(sonnet.kind, SubjectKind::Builtin);
    assert!(sonnet.available && sonnet.in_standard);
}

#[tokio::test]
async fn the_memberships_of_a_delegate_move_to_its_models_once() {
    let v = vault();
    let (device, provider_id, profile) = (v.device, Uuid::new_v4(), Uuid::new_v4());
    let agent = delegate_id(device, AgentKind::ClaudeCodeDelegate);
    v.db.write(move |tx| {
        insert_provider(tx, &provider(provider_id, "claude"))?;
        upsert_model(tx, &model(provider_id, "sonnet", "Sonnet"))?;
        upsert_model(tx, &model(provider_id, "opus", "Opus"))?;
        registry::register_delegate(
            tx,
            device,
            AgentKind::ClaudeCodeDelegate,
            "Claude Code",
            super::STANDARD_PROFILE_ID,
            AgentMode::Manual,
            1,
        )?;
        store::create_profile(tx, profile, "Streng")?;
        store::add_member(tx, &Subject::Agent(agent), profile)
    })
    .await
    .expect("setup");

    assert_eq!(v.db.write(convert_memberships).await.expect("convert"), 1);
    for remote in ["sonnet", "opus"] {
        let id = format!("{provider_id}:{remote}");
        let memberships =
            v.db.read(move |q| store::memberships(q, &Subject::Model(id.clone())))
                .await
                .expect("read");
        assert_eq!(memberships.profiles, vec![profile], "{remote} is in it");
    }
    let delegate =
        v.db.read(move |q| store::memberships(q, &Subject::Agent(agent)))
            .await
            .expect("read");
    assert!(delegate.profiles.is_empty(), "the row keeps none");
    assert_eq!(
        v.db.write(convert_memberships).await.expect("again"),
        0,
        "nothing to do the second time"
    );
}

#[tokio::test]
async fn a_delegate_without_models_keeps_its_memberships_until_there_are_some() {
    let v = vault();
    let (device, profile) = (v.device, Uuid::new_v4());
    let agent = delegate_id(device, AgentKind::CodexDelegate);
    v.db.write(move |tx| {
        registry::register_delegate(
            tx,
            device,
            AgentKind::CodexDelegate,
            "Codex",
            super::STANDARD_PROFILE_ID,
            AgentMode::Manual,
            1,
        )?;
        store::create_profile(tx, profile, "Streng")?;
        store::add_member(tx, &Subject::Agent(agent), profile)
    })
    .await
    .expect("setup");
    assert_eq!(v.db.write(convert_memberships).await.expect("convert"), 0);
    let kept =
        v.db.read(move |q| store::memberships(q, &Subject::Agent(agent)))
            .await
            .expect("read");
    assert_eq!(kept.profiles, vec![profile]);
}

mod autonomy_settings {
    use super::*;
    use crate::adapters::cli_delegate::autonomy::{AutonomyMode, DenyCategory};
    use crate::agent_profiles::delegates::convert_autonomy_settings;
    use crate::agent_profiles::{resolve, STANDARD_PROFILE_ID};
    use crate::storage::preferences::{self, PrefScope};

    async fn standard_delegate(
        v: &super::super::test_support::Vault,
    ) -> crate::mcp_server::grants::DelegateRights {
        v.db.read(|q| resolve::delegate_rights(q, "any-model"))
            .await
            .expect("read")
    }

    #[tokio::test]
    async fn a_vault_without_the_settings_keeps_the_factory_value_and_writes_nothing() {
        let v = vault();
        let device = v.device;
        assert!(!v
            .db
            .write(move |tx| convert_autonomy_settings(tx, device))
            .await
            .expect("convert"));
        assert_eq!(
            standard_delegate(&v).await.autonomy,
            AutonomyMode::Ungated,
            "the factory value is today's default"
        );
        let standard =
            v.db.read(|q| store::profile(q, STANDARD_PROFILE_ID))
                .await
                .expect("read");
        assert!(standard.is_none(), "Standard has no row of its own");
    }

    #[tokio::test]
    async fn the_settings_become_the_value_of_standard_once_and_are_removed() {
        let v = vault();
        let device = v.device;
        v.db.write(|tx| {
            preferences::insert_or_update(
                tx,
                PrefScope::Vault,
                "chat.autonomy_mode",
                "gated_permissive",
            )?;
            preferences::insert_or_update(
                tx,
                PrefScope::Vault,
                "cli_delegate.deny_rules",
                r#"["network_access","credential_paths"]"#,
            )?;
            Ok(())
        })
        .await
        .expect("the old settings");
        assert!(v
            .db
            .write(move |tx| convert_autonomy_settings(tx, device))
            .await
            .expect("convert"));
        let delegate = standard_delegate(&v).await;
        assert_eq!(delegate.autonomy, AutonomyMode::GatedPermissive);
        assert_eq!(
            delegate.deny.iter().copied().collect::<Vec<_>>(),
            vec![DenyCategory::NetworkAccess, DenyCategory::CredentialPaths]
        );
        // Nothing else of Standard changed: it still reads "holzi bedienen" at read.
        let rights =
            v.db.read(|q| resolve::rights_of(q, &Subject::Model("any-model".to_owned())))
                .await
                .expect("read");
        assert_eq!(
            rights.operate,
            crate::mcp_server::grants::default_rights().operate
        );
        let left =
            v.db.read(|q| preferences::get(q, PrefScope::Vault, "chat.autonomy_mode"))
                .await
                .expect("read");
        assert_eq!(left, None, "the old setting is gone");
        assert!(
            !v.db
                .write(move |tx| convert_autonomy_settings(tx, device))
                .await
                .expect("again"),
            "the second open has nothing to do"
        );
    }

    #[tokio::test]
    async fn a_value_this_build_cannot_read_becomes_the_group_that_asks_for_every_call() {
        let v = vault();
        let device = v.device;
        v.db.write(|tx| {
            preferences::insert_or_update(tx, PrefScope::Vault, "chat.autonomy_mode", "ungated")?;
            preferences::insert_or_update(
                tx,
                PrefScope::Vault,
                "cli_delegate.deny_rules",
                "not json",
            )?;
            Ok(())
        })
        .await
        .expect("the old settings");
        v.db.write(move |tx| convert_autonomy_settings(tx, device))
            .await
            .expect("convert");
        let delegate = standard_delegate(&v).await;
        assert_eq!(
            delegate.autonomy,
            AutonomyMode::Standard,
            "never a wider posture"
        );
        assert!(delegate.deny.is_empty());
    }
}
