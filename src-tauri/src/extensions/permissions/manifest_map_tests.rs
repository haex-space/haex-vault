use serde_json::json;

use super::*;
use crate::extensions::bundle::{ManifestTool, ToolEffect};
use crate::extensions::permissions::{Action, PermissionKind, Target};

fn declared(permissions: serde_json::Value) -> DeclaredPermissions {
    map_manifest_permissions(&permissions)
}

fn kinds_and_actions(result: &DeclaredPermissions) -> Vec<(PermissionKind, Action, String)> {
    result
        .declared
        .iter()
        .map(|d| (d.kind, d.action.clone(), d.target_text.clone()))
        .collect()
}

// The manifests below are the `permissions` objects of the apps in haex-space/haextension at
// db48f9a948522c18a00331aac232718825cc9317 (`apps/<app>/haextension/manifest.json`).

#[test]
fn haex_notes() {
    let result = declared(json!({
        "database": [], "filesystem": [{ "target": "*", "operation": "read" }], "shell": [],
        "http": [], "spaces": [{ "target": "*", "operation": "readWrite" }]
    }));
    assert_eq!(
        kinds_and_actions(&result),
        vec![(PermissionKind::Filesystem, Action::Read, "*".to_string())]
    );
    assert_eq!(result.unsupported_categories, vec!["spaces".to_string()]);
    assert!(result.invalid.is_empty());
}

#[test]
fn haex_calendar() {
    let result = declared(json!({
        "database": [], "filesystem": [], "shell": [], "http": [],
        "passwords": [{ "target": "haex-calendar", "operation": "read_write" }],
        "spaces": [{ "target": "*", "operation": "readWrite" }],
        "notifications": [{ "target": "*", "operation": "show" }]
    }));
    assert_eq!(
        kinds_and_actions(&result),
        vec![
            (
                PermissionKind::Passwords,
                Action::ReadWrite,
                "haex-calendar".to_string()
            ),
            (PermissionKind::Notifications, Action::Show, "*".to_string()),
        ]
    );
    assert_eq!(result.unsupported_categories, vec!["spaces".to_string()]);
}

#[test]
fn haex_pass() {
    let result = declared(json!({
        "database": [], "filesystem": [],
        "http": [{ "target": "https://icons.duckduckgo.com/*" }], "shell": []
    }));
    assert_eq!(
        kinds_and_actions(&result),
        vec![(
            PermissionKind::Web,
            Action::AnyMethod,
            "https://icons.duckduckgo.com/*".to_string()
        )]
    );
    assert!(matches!(result.declared[0].target, Target::Url(_)));
}

#[test]
fn haex_files_uses_action_instead_of_operation() {
    let result = declared(json!({
        "database": [], "filesystem": [{ "target": "*", "action": "readWrite" }], "shell": []
    }));
    assert_eq!(
        kinds_and_actions(&result),
        vec![(
            PermissionKind::Filesystem,
            Action::ReadWrite,
            "*".to_string()
        )]
    );
}

#[test]
fn spellings_are_unified() {
    let result = declared(json!({
        "web": [{ "target": "example.org", "operation": "get" }],
        "cloudStorage": [{ "target": "conn-1", "operation": "read_write" }],
        "remoteStorage": [{ "target": "conn-2" }],
        "shell": [{ "target": "/bin/sh" }],
        "mail": [{ "target": "imap.example.org:993", "operation": "fetch" }]
    }));
    assert_eq!(
        kinds_and_actions(&result),
        vec![
            (
                PermissionKind::Web,
                Action::Method("GET".to_string()),
                "example.org".to_string()
            ),
            (
                PermissionKind::RemoteStorage,
                Action::ReadWrite,
                "conn-1".to_string()
            ),
            (
                PermissionKind::RemoteStorage,
                Action::Read,
                "conn-2".to_string()
            ),
            (
                PermissionKind::Shell,
                Action::Execute,
                "/bin/sh".to_string()
            ),
            (
                PermissionKind::Mail,
                Action::Fetch,
                "imap.example.org:993".to_string()
            ),
        ]
    );
}

#[test]
fn categories_holzi_does_not_offer_are_listed_and_never_declared() {
    let result = declared(json!({
        "spaces": [{ "target": "*" }], "identities": [{ "target": "*" }],
        "bookmarks": [{ "target": "*" }], "syncServers": [{ "target": "*" }],
        "syncRules": [{ "target": "*" }], "teleport": [{ "target": "*" }],
        "bookmarksEmpty": []
    }));
    assert!(result.declared.is_empty());
    assert_eq!(
        result.unsupported_categories,
        vec![
            "bookmarks".to_string(),
            "identities".to_string(),
            "spaces".to_string(),
            "syncRules".to_string(),
            "syncServers".to_string(),
            "teleport".to_string(),
        ]
    );
}

#[test]
fn invalid_declarations_are_reported_and_not_declared() {
    let result = declared(json!({
        "database": [{ "target": "chat_*", "operation": "read" }],
        "filesystem": [{ "target": "relative", "operation": "read" }, { "operation": "read" }],
        "mail": [{ "target": "imap.example.org" }],
        "notifications": "all"
    }));
    assert!(result.declared.is_empty());
    assert_eq!(result.invalid.len(), 5);
    assert!(result
        .invalid
        .iter()
        .any(|i| i.category == "database" && i.target.as_deref() == Some("chat_*")));
}

#[test]
fn a_missing_permissions_object_declares_nothing() {
    let result = declared(serde_json::Value::Null);
    assert!(result.declared.is_empty() && result.unsupported_categories.is_empty());
}

#[test]
fn encrypted_folders_are_forbidden_in_every_spelling_and_never_declared() {
    let result = declared(json!({
        "encryptedFolder": [{ "target": "s1/aaaaaaaaaaaaaaaaaaaaaaaaaa", "operation": "read" }],
        "Encrypted_Folders": [],
        "remoteStorage": [{ "target": "s1" }]
    }));
    assert_eq!(
        result.forbidden_categories,
        ["Encrypted_Folders", "encryptedFolder"]
    );
    assert_eq!(
        kinds_and_actions(&result),
        vec![(
            PermissionKind::RemoteStorage,
            Action::Read,
            "s1".to_string()
        )]
    );
    assert!(result.unsupported_categories.is_empty());
    assert!(result.invalid.is_empty());
}

fn tool(name: &str, effect: ToolEffect) -> ManifestTool {
    ManifestTool {
        name: name.into(),
        title_de: name.into(),
        title_en: name.into(),
        description: name.into(),
        input_schema: json!({ "type": "object" }),
        effect,
        examples_de: vec![name.into()],
        examples_en: vec![name.into()],
        supported: true,
    }
}

#[test]
fn each_tool_becomes_an_agent_tool_permission_with_its_effect() {
    let declared = map_manifest_tools(&[
        tool("count_unread", ToolEffect::Read),
        tool("delete_all", ToolEffect::Destructive),
    ]);
    let rows: Vec<_> = declared
        .iter()
        .map(|d| (d.kind, d.action.clone(), d.target_text.as_str()))
        .collect();
    assert_eq!(
        rows,
        [
            (PermissionKind::AgentTool, Action::Read, "count_unread"),
            (PermissionKind::AgentTool, Action::Destructive, "delete_all"),
        ]
    );
    assert_eq!(declared[0].target, Target::Tool("count_unread".into()));
}

#[test]
fn tools_are_declared_only_in_the_tools_block() {
    let result = declared(json!({ "agentTool": [{ "target": "*", "action": "read" }] }));
    assert!(result.declared.is_empty());
    assert_eq!(result.unsupported_categories, ["agentTool"]);
}
