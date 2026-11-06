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
        .write(move |tx| store::assign(tx, &Subject::Agent(first_id), nothing))
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

    // Back to "Standard": the factory rights again.
    let mut changes = first.service().list_changes();
    fx.db
        .write(move |tx| store::assign(tx, &Subject::Agent(first_id), STANDARD_PROFILE_ID))
        .await
        .expect("assigned back");
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
                    member_id(&Subject::Agent(second_id)),
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
