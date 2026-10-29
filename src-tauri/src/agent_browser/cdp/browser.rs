//! The `Browser` domain: the methods holzi calls on the browser itself (contracts/agent-browser.md
//! "Start", research R1).

use serde_json::{json, Value};

use super::Connection;
use crate::agent_browser::error::{BrowserError, Result};

/// The page permissions holzi denies in every agent tab (FR-026), by their web names
/// (`PermissionDescriptor.name`).
pub const DENIED: [&str; 6] = [
    "camera",
    "microphone",
    "geolocation",
    "notifications",
    "clipboard-read",
    "clipboard-write",
];

/// `Browser.getVersion`: the product, `Chrome/156.0.8078.73`.
pub async fn product(connection: &Connection) -> Result<String> {
    let version = connection
        .call("Browser.getVersion", json!({}), None)
        .await?;
    version
        .get("product")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| BrowserError::Protocol("no product in Browser.getVersion".to_owned()))
}

/// `Browser.setPermission` to `denied` for every permission of [`DENIED`].
pub async fn deny_permissions(connection: &Connection) -> Result<()> {
    for name in DENIED {
        connection
            .call(
                "Browser.setPermission",
                json!({ "permission": { "name": name }, "setting": "denied" }),
                None,
            )
            .await?;
    }
    Ok(())
}

/// `Browser.setDownloadBehavior`: downloads land in `dir` under their GUID (research R11).
pub async fn download_into(connection: &Connection, dir: &str) -> Result<()> {
    connection
        .call(
            "Browser.setDownloadBehavior",
            json!({ "behavior": "allowAndName", "downloadPath": dir, "eventsEnabled": true }),
            None,
        )
        .await
        .map(drop)
}

/// `Browser.close`: the browser ends in order and writes its profile.
pub async fn close(connection: &Connection) -> Result<()> {
    connection
        .call("Browser.close", json!({}), None)
        .await
        .map(drop)
}
