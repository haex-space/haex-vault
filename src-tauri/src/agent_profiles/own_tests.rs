//! Tests for the interim write path (spec 083 research R8): rights written through
//! `registry::save_rights` land in a profile of the agent's own and change no other agent.

use super::resolve::normalized;
use super::test_support::{registered, vault};
use super::{own_profile_id, store, Subject, STANDARD_PROFILE_ID};
use crate::mcp_server::grants::{default_rights, BrowserRights, Rights};
use crate::mcp_server::manage;
use crate::mcp_server::registry::{load_rights, save_rights};

fn with_browser() -> Rights {
    Rights {
        browser: Some(BrowserRights::default()),
        ..default_rights()
    }
}

#[tokio::test]
async fn rights_other_than_standards_go_into_a_profile_of_the_agents_own() {
    let v = vault();
    let (device, one, two) = (
        v.device,
        registered(&v, "claude-code").await,
        registered(&v, "codex").await,
    );
    let wanted = with_browser();
    let w = wanted.clone();
    v.db.write(move |tx| save_rights(tx, device, one, &w))
        .await
        .expect("save");
    let (mine, theirs, profile) =
        v.db.read(move |q| {
            Ok((
                load_rights(q, device, one)?,
                load_rights(q, device, two)?,
                store::profile(q, own_profile_id(device, one))?,
            ))
        })
        .await
        .expect("read");
    assert_eq!(normalized(&mine), normalized(&wanted));
    assert_eq!(theirs, default_rights(), "another agent is unchanged");
    assert_eq!(profile.map(|p| p.name), Some("claude-code".to_owned()));
}

#[tokio::test]
async fn rights_equal_to_standard_leave_the_agent_unassigned_and_drop_its_own_profile() {
    let v = vault();
    let (device, agent) = (v.device, registered(&v, "claude-code").await);
    let w = with_browser();
    v.db.write(move |tx| save_rights(tx, device, agent, &w))
        .await
        .expect("save");
    v.db.write(move |tx| save_rights(tx, device, agent, &default_rights()))
        .await
        .expect("back to the factory rights");
    let (member, profile) =
        v.db.read(move |q| {
            Ok((
                store::member_profile(q, &Subject::Agent(agent))?,
                store::profile(q, own_profile_id(device, agent))?,
            ))
        })
        .await
        .expect("read");
    assert_eq!(member, None);
    assert!(
        profile.is_none(),
        "nothing keeps the agent's own profile alive"
    );
}

#[tokio::test]
async fn saving_what_the_agent_has_changes_nothing() {
    let v = vault();
    let (device, agent) = (v.device, registered(&v, "claude-code").await);
    let profile = uuid::Uuid::new_v4();
    v.db.write(move |tx| {
        store::create_profile(tx, profile, "Geteilt")?;
        store::set_rights(tx, profile, &with_browser())?;
        store::assign(tx, &Subject::Agent(agent), profile)
    })
    .await
    .expect("shared profile");
    // A save of the rights the agent already has (a remembered answer that changed nothing, a
    // reopened page) must not move it out of the shared profile.
    v.db.write(move |tx| save_rights(tx, device, agent, &with_browser()))
        .await
        .expect("save");
    let member =
        v.db.read(move |q| store::member_profile(q, &Subject::Agent(agent)))
            .await
            .expect("read");
    assert_eq!(member, Some(profile.to_string()));
}

#[tokio::test]
async fn a_standard_that_was_edited_is_what_equal_rights_are_measured_against() {
    let v = vault();
    let (device, agent) = (v.device, registered(&v, "claude-code").await);
    let wide = with_browser();
    let w = wide.clone();
    v.db.write(move |tx| store::set_rights(tx, STANDARD_PROFILE_ID, &w))
        .await
        .expect("edit Standard");
    v.db.write(move |tx| save_rights(tx, device, agent, &wide))
        .await
        .expect("save");
    let member =
        v.db.read(move |q| store::member_profile(q, &Subject::Agent(agent)))
            .await
            .expect("read");
    assert_eq!(member, None, "the agent already has what Standard gives");
}

#[tokio::test]
async fn revoking_an_agent_removes_its_assignment_and_its_own_profile() {
    let v = vault();
    let (device, agent) = (v.device, registered(&v, "claude-code").await);
    let w = with_browser();
    v.db.write(move |tx| save_rights(tx, device, agent, &w))
        .await
        .expect("save");
    let removed =
        v.db.write(move |tx| manage::revoke(tx, device, agent))
            .await
            .expect("revoke");
    assert!(removed);
    let (member, profile) =
        v.db.read(move |q| {
            Ok((
                store::member_profile(q, &Subject::Agent(agent))?,
                store::profile(q, own_profile_id(device, agent))?,
            ))
        })
        .await
        .expect("read");
    assert!(member.is_none() && profile.is_none());
}

#[tokio::test]
async fn revoking_keeps_an_own_profile_that_others_were_put_into() {
    let v = vault();
    let (device, agent, other) = (
        v.device,
        registered(&v, "claude-code").await,
        registered(&v, "codex").await,
    );
    let w = with_browser();
    v.db.write(move |tx| save_rights(tx, device, agent, &w))
        .await
        .expect("save");
    v.db.write(move |tx| store::assign(tx, &Subject::Agent(other), own_profile_id(device, agent)))
        .await
        .expect("a second agent joins");
    v.db.write(move |tx| manage::revoke(tx, device, agent))
        .await
        .expect("revoke");
    let profile =
        v.db.read(move |q| store::profile(q, own_profile_id(device, agent)))
            .await
            .expect("read");
    assert!(profile.is_some(), "the other agent still uses it");
}
