//! Spec 083 US7 (T063, FR-024, FR-025, SC-004): after the update nothing from before counts. There
//! is no migration: the old rows of `haex_mcp_agent_grants` are not read, so every agent has exactly
//! the factory rights of "Standard"; its registration stays; the local agent's two old settings
//! are deleted; a profile the user made after the update survives a second run of the cleanup.

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

fn old_row(
    agent: Uuid,
    device: Uuid,
    group: &str,
    key: &str,
    level: &str,
) -> (String, String, String, String, String, String) {
    (
        Uuid::new_v4().to_string(),
        device.to_string(),
        agent.to_string(),
        group.to_owned(),
        key.to_owned(),
        level.to_owned(),
    )
}

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
        mode: AgentMode::Plan,
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
            // What the agents were allowed before: a browser, passwords, an extension, commands.
            for owner in [agent.id, delegate.id] {
                for (group, key, level) in [
                    ("operate", "chat.write", "write"),
                    ("browser", "", "on"),
                    ("browser", "https://shop.example.de", "operate"),
                    ("passwords", "", "read_write"),
                    ("extension", "00000000-0000-0000-0000-000000004d41", "on"),
                    ("commands", "", "on"),
                ] {
                    let row = old_row(owner, device, group, key, level);
                    tx.execute(
                        "INSERT INTO haex_mcp_agent_grants \
                         (id, vault_device_uuid, agent_id, grant_group, grant_key, level) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![row.0, row.1, row.2, row.3, row.4, row.5],
                    )?;
                }
            }
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

    for id in [agent.id, delegate.id] {
        let rights = fx
            .db
            .read(move |q| registry::load_rights(q, device, id))
            .await
            .expect("rights");
        assert_eq!(
            normalized(&rights),
            normalized(&default_rights()),
            "factory rights only"
        );
    }
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
        (AgentMode::Plan, "old-agent")
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
            store::assign(tx, &s, profile)
        })
        .await
        .expect("a profile");
    fx.db
        .write(delete_local_agent_rights)
        .await
        .expect("the cleanup once more");
    let assigned = fx
        .db
        .read(move |q| store::member_profile(q, &subject))
        .await
        .expect("read");
    assert_eq!(assigned, Some(profile.to_string()));
}
