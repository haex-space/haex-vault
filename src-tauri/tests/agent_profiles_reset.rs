//! Spec 083 US7 (T063, FR-024, FR-025, SC-004): every agent has exactly the factory rights of
//! "Standard"; its registration stays; the local agent's two old settings are deleted; a profile
//! the user made after the update survives a second run of the cleanup.

#[path = "common/mcp_fixture.rs"]
mod mcp_fixture;

use haex_crdt::rusqlite::params;
use mcp_fixture::*;
use uuid::Uuid;

use holzi_lib::agent_profiles::resolve::normalized;
use holzi_lib::agent_profiles::{store, Subject};
use holzi_lib::mcp_server::delegates::delegate_id;
use holzi_lib::mcp_server::grants::default_rights;
use holzi_lib::mcp_server::registry::{self, AgentKind, AgentMode, NewAgent, Transport};
use holzi_lib::storage::maintenance::delete_local_agent_rights;
use holzi_lib::storage::preferences::{self, PrefScope};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_agent_starts_in_standard_and_the_old_settings_are_gone() {
    let fx = fixture();
    let device = fx.device;
    let new = NewAgent {
        kind: AgentKind::External,
        slug: "old-agent".to_owned(),
        client_name: "Old agent".to_owned(),
        client_version: None,
        transport: Transport::Ipc,
    };
    let (agent, delegate) = fx
        .db
        .write(move |tx| {
            let agent = registry::register(
                tx,
                device,
                &new,
                "credential-hash",
                holzi_lib::agent_profiles::STANDARD_PROFILE_ID,
                1_000,
            )?;
            let delegate = registry::register_delegate(
                tx,
                device,
                AgentKind::ClaudeCodeDelegate,
                "Claude Code",
                holzi_lib::agent_profiles::STANDARD_PROFILE_ID,
                AgentMode::Auto,
                1_000,
            )?;
            // This schema no longer has the legacy per-agent grants table. The dedicated 0056
            // migration test covers its removal from old vaults; here registration and cleanup
            // are checked against the current Standard profile.
            for scope in [PrefScope::Device(device), PrefScope::Vault] {
                preferences::insert_or_update(tx, scope, "agent_browser.builtin", "{}")?;
                preferences::insert_or_update(tx, scope, "agent_browser.builtin_passwords", "{}")?;
            }
            Ok((agent, delegate))
        })
        .await
        .expect("a vault from before the update");
    assert_eq!(
        delegate.id,
        delegate_id(device, AgentKind::ClaudeCodeDelegate)
    );

    // After the update: the cleanup runs when the vault opens, any number of times.
    for _ in 0..2 {
        fx.db
            .write(delete_local_agent_rights)
            .await
            .expect("the cleanup");
    }

    let rights = fx
        .db
        .read(move |q| registry::load_rights(q, device, agent.id))
        .await
        .expect("rights");
    assert_eq!(
        normalized(&rights),
        normalized(&default_rights()),
        "factory rights only"
    );
    // A delegate has the rights of the model of its running start (spec 083 FR-028); with none
    // running it has none, and nothing of what it had before the update is left.
    let rights = fx
        .db
        .read(move |q| registry::load_rights(q, device, delegate.id))
        .await
        .expect("rights");
    assert_eq!(rights, Default::default(), "no start, no rights");
    // The registration stays as it was.
    let stored_hash: Option<String> = fx
        .db
        .read(move |q| {
            use holzi_lib::storage::query::Query;
            q.query_row(
                "SELECT token_hash FROM haex_mcp_agents WHERE id = ?1",
                params![agent.id.to_string()],
                |row| row.get(0),
            )
        })
        .await
        .expect("read")
        .flatten();
    assert_eq!(
        stored_hash.as_deref(),
        Some("credential-hash"),
        "the credential stays"
    );
    let row = fx
        .db
        .read(move |q| registry::get(q, device, agent.id))
        .await
        .expect("read")
        .expect("the registration");
    assert_eq!(
        (row.mode, row.slug.as_str()),
        (AgentMode::Auto, "old-agent")
    );
    let left = fx
        .db
        .read(|q| preferences::list_by_key(q, "agent_browser.builtin"))
        .await
        .expect("list");
    assert!(
        left.is_empty(),
        "the old setting of the local agent is gone"
    );

    // A profile the user makes after the update is not touched by another run of the cleanup.
    let (profile, subject) = (Uuid::new_v4(), Subject::Agent(agent.id));
    let s = subject.clone();
    fx.db
        .write(move |tx| {
            store::create_profile(tx, profile, "After the update")?;
            store::add_member(tx, &s, profile)
        })
        .await
        .expect("a profile");
    fx.db
        .write(delete_local_agent_rights)
        .await
        .expect("the cleanup once more");
    let assigned = fx
        .db
        .read(move |q| store::memberships(q, &subject))
        .await
        .expect("read");
    assert_eq!(assigned.profiles, vec![profile]);
}
