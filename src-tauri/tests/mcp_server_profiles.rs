//! Spec 083 US1 (T019): an agent profile reaches the agents in it at once. Two live agents start
//! in "Standard"; putting one in a profile that gives nothing shrinks its tool list and refuses
//! its calls, the other is unchanged; "Standard" gives it back what it had. A member row that
//! arrives from another device (written straight into the table, announced by the table feed)
//! refreshes the session as well.

#[path = "common/mcp_fixture.rs"]
mod mcp_fixture;

use haex_crdt::rusqlite::params;
use mcp_fixture::*;

use holzi_lib::agent_profiles::{member_id, store, Subject, STANDARD_PROFILE_ID};
use holzi_lib::mcp_server::grants::Rights;
use holzi_lib::mcp_server::hello::CloseReason;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_profile_changes_what_its_agents_can_do_at_once() {
    let mut fx = fixture();
    let server = fx.server().await;
    let (first, _first_helper) = fx.registered_in_standard("first").await;
    let (second, _second_helper) = fx.registered_in_standard("second").await;
    let (first_id, second_id) = (fx.agent_id("first").await, fx.agent_id("second").await);
    let with_standard = tool_names(&first).await;
    assert!(!with_standard.is_empty(), "Standard gives reading");
    assert_eq!(tool_names(&second).await, with_standard);

    // Into a profile that gives nothing: the list shrinks, a call is refused.
    let nothing = fx.profile(Rights::default()).await;
    let mut changes = first.service().list_changes();
    fx.db
        .write(move |tx| store::add_member(tx, &Subject::Agent(first_id), nothing))
        .await
        .expect("assigned");
    fx.announce_profiles();
    next_list_change(&mut changes).await;
    assert!(tool_names(&first).await.is_empty());
    assert_eq!(
        error_code(&call(first.peer(), "wm_state_get").await).as_deref(),
        Some("not_allowed")
    );
    assert_eq!(
        tool_names(&second).await,
        with_standard,
        "the other agent is unchanged"
    );

    // Out of the profile again: the factory rights of "Standard".
    let mut changes = first.service().list_changes();
    fx.db
        .write(move |tx| store::remove_member(tx, &Subject::Agent(first_id), nothing))
        .await
        .expect("removed again");
    fx.announce_profiles();
    next_list_change(&mut changes).await;
    assert_eq!(tool_names(&first).await, with_standard);

    // A member row as a synced commit writes it: straight into the table, then the feed.
    let mut changes = second.service().list_changes();
    fx.db
        .write(move |tx| {
            tx.execute(
                "INSERT INTO haex_agent_profile_members (id, subject, profile_id) \
                 VALUES (?1, ?2, ?3)",
                params![
                    member_id(&Subject::Agent(second_id), nothing),
                    Subject::Agent(second_id).key(),
                    nothing.to_string()
                ],
            )?;
            Ok(())
        })
        .await
        .expect("a received row");
    fx.announce_profiles();
    next_list_change(&mut changes).await;
    assert!(tool_names(&second).await.is_empty());

    // A profile deleted on another device counts as "Standard" for the agents still pointing at it.
    let mut changes = second.service().list_changes();
    fx.db
        .write(move |tx| {
            tx.execute(
                "DELETE FROM haex_agent_profiles WHERE id = ?1",
                params![nothing.to_string()],
            )?;
            Ok(())
        })
        .await
        .expect("a deleted profile");
    fx.announce_profiles();
    next_list_change(&mut changes).await;
    assert_eq!(tool_names(&second).await, with_standard);
    server.stop(CloseReason::Disabled).await;
}

/// Spec 083 and spec 065 T058: a profile that takes the browser away ends the agent's browser
/// session, as a change of its rights by hand did before.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_profile_without_the_browser_ends_the_browser_session() {
    use holzi_lib::mcp_server::grants::{BrowserRights, OperateLevel};
    use holzi_lib::mcp_server::server::Notice;

    let mut fx = fixture();
    let server = fx.server().await;
    let browsing = Rights {
        operate: [("wm.read".to_owned(), OperateLevel::Read)]
            .into_iter()
            .collect(),
        browser: Some(BrowserRights::default()),
        ..Rights::default()
    };
    let (agent, _helper) = fx.registered("browsing", browsing).await;
    let id = fx.agent_id("browsing").await;

    let mut changes = agent.service().list_changes();
    fx.give_rights(id, Rights::default()).await;
    next_list_change(&mut changes).await;
    let lost = {
        let notices = fx.notices.lock().expect("notices");
        notices
            .iter()
            .any(|notice| matches!(notice, Notice::BrowserRightLost(lost) if *lost == id))
    };
    assert!(lost, "the browser session ends");
    server.stop(CloseReason::Disabled).await;
}

/// Spec 083 FR-006a: the active profile applies to the agents that are members of it. A member of
/// two profiles has the rights of the one that is active, and making another active (a change of
/// the preference, announced by the table feed) changes the live session's tools at once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn making_a_profile_active_changes_the_tools_of_its_members() {
    use holzi_lib::mcp_server::grants::OperateLevel;

    let mut fx = fixture();
    let server = fx.server().await;
    let (agent, _helper) = fx.registered_in_standard("member").await;
    let id = fx.agent_id("member").await;
    let with_standard = tool_names(&agent).await;
    assert!(!with_standard.is_empty(), "Standard gives reading");

    // One model in a sensitive and an open profile; the fixture activates the one it makes last.
    let sensitive = fx.profile(Rights::default()).await;
    let open = fx
        .profile(Rights {
            operate: [("settings.read".to_owned(), OperateLevel::Read)]
                .into_iter()
                .collect(),
            ..Rights::default()
        })
        .await;
    fx.db
        .write(move |tx| {
            store::add_member(tx, &Subject::Agent(id), sensitive)?;
            store::add_member(tx, &Subject::Agent(id), open)
        })
        .await
        .expect("a member of both");
    let mut changes = agent.service().list_changes();
    fx.announce_profiles();
    next_list_change(&mut changes).await;
    let with_open = tool_names(&agent).await;
    assert!(
        with_open.contains(&"settings_get".to_owned()),
        "{with_open:?}"
    );
    assert!(
        with_open.len() < with_standard.len(),
        "only what the open profile gives"
    );

    let mut changes = agent.service().list_changes();
    fx.db
        .write(move |tx| store::set_active(tx, sensitive))
        .await
        .expect("activate the sensitive one");
    fx.announce_profiles();
    next_list_change(&mut changes).await;
    assert!(
        tool_names(&agent).await.is_empty(),
        "the sensitive one gives nothing"
    );

    let mut changes = agent.service().list_changes();
    fx.db
        .write(|tx| store::set_active(tx, STANDARD_PROFILE_ID))
        .await
        .expect("activate Standard");
    fx.announce_profiles();
    next_list_change(&mut changes).await;
    assert_eq!(tool_names(&agent).await, with_standard);
    server.stop(CloseReason::Disabled).await;
}
