//! Tests for the agent log and what it shows of a call (spec 021 T046, FR-028, research R10),
//! against a real temporary vault.

use std::sync::Arc;

use haex_crdt::rusqlite::params;
use serde_json::json;
use uuid::Uuid;

use super::agent_log::{read, write, NewEntry, Outcome, MAX_AGE_MILLIS, MAX_ENTRIES, MAX_PAGE};
use super::redact::{redact, redact_text, target_of, MAX_VALUE_CHARS, SECRET_PLACEHOLDER};
use crate::chat::tools::action_tool::ActionEffect;
use crate::identity::{installation_id_path, read_or_mint_installation_uuid};
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::storage::query::Query;
use crate::vault_gate::{VaultDb, VaultGate};

const NOW: i64 = 1_800_000_000_000;

struct Vault {
    _dir: tempfile::TempDir,
    db: VaultDb,
    device: Uuid,
}

fn vault() -> Vault {
    let (dir, database) = open_test_vault();
    let installation =
        read_or_mint_installation_uuid(&installation_id_path(dir.path())).expect("installation");
    let device = crate::storage::query::read(&database, |q| {
        known_devices::get_vault_device_uuid(q, installation)
    })
    .expect("read")
    .expect("this device");
    let db = VaultGate::new()
        .vault_db(Arc::new(database))
        .expect("a vault handle");
    Vault {
        _dir: dir,
        db,
        device,
    }
}

fn entry(tool: &str, outcome: Outcome, asked: bool) -> NewEntry {
    NewEntry {
        tool: tool.to_owned(),
        effect: Some(ActionEffect::Write),
        target: Some("tab-1".to_owned()),
        input: Some(r#"{"scheme":"dark"}"#.to_owned()),
        outcome,
        asked,
    }
}

async fn write_at(vault: &Vault, agent: Uuid, entry: NewEntry, at: i64) {
    let device = vault.device;
    vault
        .db
        .write(move |tx| write(tx, device, agent, &entry, at))
        .await
        .expect("write");
}

async fn read_at(
    vault: &Vault,
    agent: Uuid,
    before: Option<i64>,
    before_id: Option<String>,
    limit: u32,
    at: i64,
) -> Vec<String> {
    let device = vault.device;
    vault
        .db
        .write(move |tx| read(tx, device, agent, before, before_id.as_deref(), limit, at))
        .await
        .expect("read")
        .into_iter()
        .map(|entry| entry.tool)
        .collect()
}

// ---------------------------------------------------------------------------
// What a call shows
// ---------------------------------------------------------------------------

#[test]
fn a_field_the_schema_marks_secret_is_never_shown() {
    let schema = json!({
        "type": "object",
        "properties": {
            "entryId": { "type": "string" },
            "secret": { "type": "string", "x-holzi-secret": true },
            "nested": {
                "type": "object",
                "properties": { "pin": { "type": "string", "x-holzi-secret": true } }
            }
        }
    });
    let input = json!({ "entryId": "e-1", "secret": "hunter2", "nested": { "pin": "1234" } });
    let shown = redact(&schema, &input);
    assert_eq!(
        shown,
        json!({
            "entryId": "e-1",
            "secret": SECRET_PLACEHOLDER,
            "nested": { "pin": SECRET_PLACEHOLDER }
        })
    );
    assert!(!shown.to_string().contains("hunter2"));
}

#[test]
fn credentials_in_a_raw_command_are_hidden_without_a_schema_mark() {
    let cases = [
        ("deploy --token=abc", "deploy --token=•••"),
        ("deploy --token abc now", "deploy --token ••• now"),
        ("GITHUB_TOKEN=abc gh pr list", "GITHUB_TOKEN=••• gh pr list"),
        (
            "mysql --password=s3cret -u root",
            "mysql --password=••• -u root",
        ),
        (
            "curl -H 'Authorization: Bearer xyz' https://a.example",
            "curl -H 'Authorization: Bearer ••• https://a.example",
        ),
        (
            "git clone https://me:pw@git.example/r.git",
            "git clone https://me:•••@git.example/r.git",
        ),
        ("curl -H x-api-key: k1 url", "curl -H x-api-key: ••• url"),
        ("ls -la  /tmp", "ls -la  /tmp"),
    ];
    for (raw, shown) in cases {
        assert_eq!(redact_text(raw), shown, "{raw}");
    }
}

#[test]
fn an_argument_after_a_secret_flag_is_hidden_in_an_argument_list() {
    let input =
        json!({ "command": "deploy", "args": ["--token", "abc", "--env", "prod", "--api-key=k"] });
    let shown = redact(&json!({}), &input);
    assert_eq!(
        shown["args"],
        json!([
            "--token",
            SECRET_PLACEHOLDER,
            "--env",
            "prod",
            "--api-key=•••"
        ])
    );
    assert!(!shown.to_string().contains("abc"));
}

#[test]
fn a_long_value_is_cut() {
    let long = "x".repeat(MAX_VALUE_CHARS * 2);
    let shown = redact(&json!({}), &json!({ "text": long }));
    let kept = shown["text"].as_str().expect("text");
    assert_eq!(kept.chars().count(), MAX_VALUE_CHARS);
    assert!(kept.ends_with('…'));
}

#[test]
fn the_target_is_the_first_naming_field() {
    assert_eq!(
        target_of(&json!({ "scheme": "dark", "tabId": "t-1" })).as_deref(),
        Some("t-1")
    );
    assert_eq!(
        target_of(&json!({ "path": "/a", "id": 7 })).as_deref(),
        Some("7")
    );
    assert_eq!(target_of(&json!({ "scheme": "dark" })), None);
}

// ---------------------------------------------------------------------------
// The log
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_entry_keeps_what_the_call_did_newest_first() {
    let vault = vault();
    let agent = Uuid::new_v4();
    write_at(&vault, agent, entry("first", Outcome::Ok, false), NOW - 2).await;
    write_at(
        &vault,
        agent,
        entry("second", Outcome::DeniedUser, true),
        NOW - 1,
    )
    .await;
    write_at(
        &vault,
        Uuid::new_v4(),
        entry("other", Outcome::Ok, false),
        NOW,
    )
    .await;

    let device = vault.device;
    let entries = vault
        .db
        .write(move |tx| read(tx, device, agent, None, None, 10, NOW))
        .await
        .expect("read");
    assert_eq!(entries.len(), 2, "only this agent");
    assert_eq!(entries[0].tool, "second");
    assert_eq!(entries[0].outcome, Outcome::DeniedUser);
    assert!(entries[0].asked);
    assert_eq!(entries[0].effect, "write");
    assert_eq!(entries[0].target.as_deref(), Some("tab-1"));
    assert_eq!(entries[1].outcome, Outcome::Ok);
    assert!(!entries[1].asked);

    assert_eq!(
        read_at(&vault, agent, Some(NOW - 1), None, 10, NOW).await,
        ["first"]
    );
    assert_eq!(read_at(&vault, agent, None, None, 1, NOW).await, ["second"]);
}

#[tokio::test]
async fn pages_do_not_skip_entries_with_the_same_timestamp() {
    let vault = vault();
    let agent = Uuid::new_v4();
    write_at(&vault, agent, entry("same-a", Outcome::Ok, false), NOW).await;
    write_at(&vault, agent, entry("same-b", Outcome::Ok, false), NOW).await;
    write_at(&vault, agent, entry("same-c", Outcome::Ok, false), NOW).await;
    let device = vault.device;
    let first = vault
        .db
        .write(move |tx| read(tx, device, agent, None, None, 2, NOW))
        .await
        .expect("first page");
    assert_eq!(first.len(), 2);
    let cursor = first.last().expect("cursor");
    let before = cursor.created_at;
    let before_id = cursor.id.clone();
    let device = vault.device;
    let second = vault
        .db
        .write(move |tx| read(tx, device, agent, Some(before), Some(&before_id), 2, NOW))
        .await
        .expect("second page");
    assert_eq!(second.len(), 1);
    assert!(!first.iter().any(|entry| entry.id == second[0].id));
}

#[tokio::test]
async fn every_outcome_is_stored_and_read_back() {
    let vault = vault();
    let agent = Uuid::new_v4();
    let outcomes = [
        Outcome::Ok,
        Outcome::DeniedRights,
        Outcome::DeniedUser,
        Outcome::DeniedPlan,
        Outcome::NoAnswer,
        Outcome::Error,
        Outcome::Unavailable,
        Outcome::TooMany,
    ];
    for (at, outcome) in (0_i64..).zip(outcomes) {
        write_at(
            &vault,
            agent,
            entry(outcome.as_str(), outcome, false),
            NOW + at,
        )
        .await;
    }
    let device = vault.device;
    let read_back: Vec<Outcome> = vault
        .db
        .write(move |tx| read(tx, device, agent, None, None, MAX_PAGE, NOW + 10))
        .await
        .expect("read")
        .into_iter()
        .rev()
        .map(|entry| entry.outcome)
        .collect();
    assert_eq!(read_back, outcomes);
}

#[tokio::test]
async fn entries_older_than_thirty_days_go_on_read_too() {
    let vault = vault();
    let agent = Uuid::new_v4();
    write_at(&vault, agent, entry("old", Outcome::Ok, false), NOW).await;
    write_at(&vault, agent, entry("new", Outcome::Ok, false), NOW + 1000).await;
    let later = NOW + MAX_AGE_MILLIS + 1;
    assert_eq!(read_at(&vault, agent, None, None, 10, later).await, ["new"]);
    // Gone from the table, not only from the page.
    assert_eq!(
        read_at(&vault, agent, None, None, 10, NOW + 2000).await,
        ["new"]
    );
}

#[tokio::test]
async fn the_log_keeps_ten_thousand_entries_per_agent() {
    let vault = vault();
    let agent = Uuid::new_v4();
    let other = Uuid::new_v4();
    let device = vault.device;
    vault
        .db
        .write(move |tx| {
            for at in 0..MAX_ENTRIES + 5 {
                tx.execute(
                    "INSERT INTO haex_mcp_agent_log_no_sync (id, vault_device_uuid, agent_id, \
                     created_at, tool, effect, outcome) VALUES (?1, ?2, ?3, ?4, 'bulk', 'read', 'ok')",
                    params![
                        Uuid::new_v4().to_string(),
                        device.to_string(),
                        agent.to_string(),
                        NOW + at
                    ],
                )?;
            }
            Ok(())
        })
        .await
        .expect("fill");
    write_at(&vault, other, entry("other", Outcome::Ok, false), NOW).await;
    write_at(
        &vault,
        agent,
        entry("newest", Outcome::Ok, false),
        NOW + MAX_ENTRIES + 10,
    )
    .await;

    let count = |agent: Uuid| {
        let db = vault.db.clone();
        async move {
            db.read(move |q| {
                q.query_row(
                    "SELECT COUNT(*) FROM haex_mcp_agent_log_no_sync WHERE agent_id = ?1",
                    params![agent.to_string()],
                    |row| row.get::<_, i64>(0),
                )
            })
            .await
            .expect("count")
            .unwrap_or(0)
        }
    };
    assert_eq!(count(agent).await, MAX_ENTRIES);
    assert_eq!(count(other).await, 1, "another agent keeps its own");
    assert_eq!(
        read_at(&vault, agent, None, None, 1, NOW + MAX_ENTRIES + 20).await,
        ["newest"]
    );
}

#[tokio::test]
async fn a_page_holds_at_most_two_hundred_entries() {
    let vault = vault();
    let agent = Uuid::new_v4();
    for at in 0..(i64::from(MAX_PAGE) + 5) {
        write_at(&vault, agent, entry("call", Outcome::Ok, false), NOW + at).await;
    }
    let page = read_at(&vault, agent, None, None, 10_000, NOW + 1000).await;
    assert_eq!(page.len(), MAX_PAGE as usize);
}
