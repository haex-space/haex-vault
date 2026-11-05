//! Tests for step 1 of the post-open migration (spec 083 T014, FR-024, FR-025).

use uuid::Uuid;

use super::migrate::{migrate_agents, AGENTS_MARKER};
use super::resolve::normalized;
use super::store;
use super::test_support::{registered, vault, write_legacy, Vault};
use super::{own_profile_id, Subject, STANDARD_PROFILE_ID};
use crate::mcp_server::grants::{
    default_rights, BrowserRights, OperateLevel, PasswordLevel, PasswordRights, Rights,
};
use crate::mcp_server::registry::load_rights;
use crate::storage::preferences::{self, PrefScope};

fn rich() -> Rights {
    Rights {
        operate: [
            ("wm.read".to_owned(), OperateLevel::Write),
            ("chat.read".to_owned(), OperateLevel::Read),
        ]
        .into_iter()
        .collect(),
        browser: Some(BrowserRights::default()),
        passwords: Some(PasswordRights {
            level: PasswordLevel::Read,
            tags: Some(vec!["dev".to_owned()]),
            default_tag: None,
        }),
        commands: true,
        ..Rights::default()
    }
}

async fn migrate(v: &Vault) -> usize {
    let device = v.device;
    v.db.write(move |tx| migrate_agents(tx, device))
        .await
        .expect("migrate")
}

async fn rights(v: &Vault, agent: Uuid) -> Rights {
    let device = v.device;
    v.db.read(move |q| load_rights(q, device, agent))
        .await
        .expect("read")
}

#[tokio::test]
async fn a_factory_rights_agent_gets_no_assignment() {
    let v = vault();
    let agent = registered(&v, "claude-code").await;
    write_legacy(&v, agent, default_rights()).await;
    assert_eq!(migrate(&v).await, 0);
    let member =
        v.db.read(move |q| store::member_profile(q, &Subject::Agent(agent)))
            .await
            .expect("read");
    assert_eq!(member, None);
    assert_eq!(rights(&v, agent).await, default_rights());
}

#[tokio::test]
async fn an_agent_with_its_own_rights_gets_a_profile_with_its_name_and_the_same_rights() {
    let v = vault();
    let agent = registered(&v, "claude-code").await;
    write_legacy(&v, agent, rich()).await;
    assert_eq!(migrate(&v).await, 1);
    let device = v.device;
    let profile =
        v.db.read(move |q| store::profile(q, own_profile_id(device, agent)))
            .await
            .expect("read");
    assert_eq!(profile.map(|p| p.name), Some("claude-code".to_owned()));
    assert_eq!(
        normalized(&rights(&v, agent).await),
        normalized(&rich()),
        "nothing widens, nothing narrows"
    );
}

#[tokio::test]
async fn an_agent_that_may_do_nothing_keeps_doing_nothing() {
    let v = vault();
    let agent = registered(&v, "claude-code").await;
    write_legacy(&v, agent, Rights::default()).await;
    migrate(&v).await;
    assert_eq!(rights(&v, agent).await, Rights::default());
}

#[tokio::test]
async fn a_factory_agent_gets_its_own_profile_when_standard_already_holds_other_rights() {
    let v = vault();
    let agent = registered(&v, "claude-code").await;
    write_legacy(&v, agent, default_rights()).await;
    // Another device edited "Standard" and the edit has arrived.
    v.db.write(|tx| store::set_rights(tx, STANDARD_PROFILE_ID, &rich()))
        .await
        .expect("edit Standard");
    assert_eq!(migrate(&v).await, 1);
    assert_eq!(
        normalized(&rights(&v, agent).await),
        normalized(&default_rights()),
        "it keeps the factory rights, not the wider Standard"
    );
}

#[tokio::test]
async fn two_agents_with_one_name_get_two_profiles_with_that_name() {
    let v = vault();
    let (one, two) = (
        registered(&v, "claude-code").await,
        registered(&v, "claude-code").await,
    );
    write_legacy(&v, one, rich()).await;
    write_legacy(&v, two, rich()).await;
    assert_eq!(migrate(&v).await, 2);
    let names =
        v.db.read(|q| {
            Ok(store::profiles(q)?
                .into_iter()
                .map(|p| p.name)
                .collect::<Vec<_>>())
        })
        .await
        .expect("read");
    assert_eq!(names, vec!["claude-code", "claude-code"]);
}

#[tokio::test]
async fn a_blocked_agent_is_left_alone() {
    let v = vault();
    let device = v.device;
    v.db.write(move |tx| {
        crate::mcp_server::registry::block(
            tx,
            device,
            "bad",
            "bad",
            crate::mcp_server::registry::Transport::Ipc,
            1,
        )
    })
    .await
    .expect("block");
    assert_eq!(migrate(&v).await, 0);
}

#[tokio::test]
async fn running_it_again_changes_nothing() {
    let v = vault();
    let agent = registered(&v, "claude-code").await;
    write_legacy(&v, agent, rich()).await;
    assert_eq!(migrate(&v).await, 1);
    let before = rights(&v, agent).await;
    assert_eq!(migrate(&v).await, 0, "the marker is set");
    assert_eq!(rights(&v, agent).await, before);
    let device = v.device;
    let marker =
        v.db.read(move |q| preferences::get(q, PrefScope::Device(device), AGENTS_MARKER))
            .await
            .expect("read");
    assert_eq!(marker.as_deref(), Some("1"));
}

#[tokio::test]
async fn a_stop_in_the_middle_is_repeated_without_a_second_profile() {
    let v = vault();
    let (device, agent) = (v.device, registered(&v, "claude-code").await);
    write_legacy(&v, agent, rich()).await;
    // The profile and its rows exist, the assignment and the marker do not.
    v.db.write(move |tx| {
        let own = own_profile_id(device, agent);
        store::create_profile(tx, own, "claude-code")?;
        store::set_rights(tx, own, &rich())
    })
    .await
    .expect("half done");
    assert_eq!(migrate(&v).await, 1);
    let profiles = v.db.read(|q| store::profiles(q)).await.expect("read");
    assert_eq!(
        profiles.len(),
        1,
        "the profile with the deterministic id is reused"
    );
    assert_eq!(normalized(&rights(&v, agent).await), normalized(&rich()));
}
