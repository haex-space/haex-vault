//! Wire types for the generic notifications API.
//!
//! These mirror `haex-vault-sdk/src/api/notifications.ts`. Field names use
//! camelCase to match the SDK request/response shapes 1:1.

use serde::{Deserialize, Serialize};

/// A deep link into the calling extension (an extension-internal route).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepLink {
    pub path: String,
}

/// An action button on a notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationAction {
    pub id: String,
    pub label: String,
    pub deep_link: DeepLink,
}

/// Options for `extension_notifications_show` (the SDK's `NotificationOptions`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationOptions {
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub primary: Option<DeepLink>,
    #[serde(default)]
    pub actions: Vec<NotificationAction>,
    #[serde(default)]
    pub tag: Option<String>,
}

/// Return value of `extension_notifications_show`.
#[derive(Debug, Clone, Serialize)]
pub struct ShowResult {
    pub id: String,
}

/// Payload of the `haextension:notification:click` event delivered to the
/// owning extension's webview.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationClickPayload {
    pub notification_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}
