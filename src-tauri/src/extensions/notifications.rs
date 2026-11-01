//! System notifications of extensions (spec 017, US8, T100, FR-052, contracts/bridge.md,
//! research R20).
//!
//! An extension shows a notification only with the `notifications` permission (`show`, `*`); holzi
//! gives it an id and remembers whose it is, so an extension removes only its own. A click on the
//! notification or one of its buttons reaches every open frame of the extension as
//! `haextension:notification:click {notificationId, actionId?, path?}`, and holzi's window brings
//! the extension's tab forward (`extension-notification-click`). How the system shows it and
//! whether it reports clicks is the [`Desktop`](super::host::Desktop)'s part.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use base64::Engine;
use serde_json::{json, Value};
use uuid::Uuid;

use super::bridge::dispatch::{CallContext, Emit};
use super::bridge::events::emit_to_frames;
use super::bundle::store::read_verified_file;
use super::error::{BridgeError, ExtensionErrorCode};
use super::host::ExtensionHost;
use super::permissions::store::candidates;
use super::permissions::{
    evaluate, Action, Decision, PermissionKind, PermissionRequest, RequestTarget,
};

pub const MODULE: &str = module_path!();

/// The SDK's event for a click.
pub const CLICK: &str = "haextension:notification:click";
/// Event to holzi's window: bring the extension's tab forward.
pub const CLICKED: &str = "extension-notification-click";
/// The event that shows a notification inside holzi's window, where the system may not show it
/// (spec 043 FR-023: the person refused Android's permission): `{extensionId, title, body?}`.
pub const IN_APP: &str = "extension-notification-in-app";

pub const MAX_TITLE_CHARS: usize = 200;
pub const MAX_BODY_CHARS: usize = 2000;
pub const MAX_ACTIONS: usize = 3;
pub const MAX_LABEL_CHARS: usize = 64;
pub const MAX_ID_CHARS: usize = 64;
pub const MAX_PATH_CHARS: usize = 2048;
pub const MAX_TAG_CHARS: usize = 128;
pub const MAX_ICON_BYTES: usize = 256 * 1024;
/// Notifications one extension has open at most; a new one closes the oldest. Where the system
/// reports no click or close, holzi cannot tell when one is gone.
pub const MAX_OPEN: usize = 20;

/// Image types an icon may have.
const ICON_TYPES: &[(&str, &str)] = &[
    ("image/png", "png"),
    ("image/jpeg", "jpg"),
    ("image/webp", "webp"),
    ("image/gif", "gif"),
];

pub use crate::notifications::{
    NotificationButton, NotificationResponse, NotificationSpec, Respond, ShownNotification,
};

/// Where a click leads, as the extension set it.
#[derive(Debug, Clone, Default)]
struct Links {
    primary: Option<String>,
    buttons: HashMap<String, Option<String>>,
}

struct Open {
    extension_id: Uuid,
    /// Order of showing.
    seq: u64,
    tag: Option<String>,
    /// `None` between the entry and the system's answer to showing it.
    shown: Option<Box<dyn ShownNotification>>,
}

/// The open notifications of this process, by the id holzi gave them.
#[derive(Default)]
pub struct NotificationState {
    open: Mutex<HashMap<String, Open>>,
    next: AtomicU64,
}

impl NotificationState {
    fn lock(&self) -> MutexGuard<'_, HashMap<String, Open>> {
        self.open.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn forget(&self, id: &str) -> bool {
        self.lock().remove(id).is_some()
    }

    /// The ids of the notifications of `extension_id`, oldest first.
    pub fn of_extension(&self, extension_id: Uuid) -> Vec<String> {
        let open = self.lock();
        let mut own: Vec<(&String, u64)> = open
            .iter()
            .filter(|(_, o)| o.extension_id == extension_id)
            .map(|(id, o)| (id, o.seq))
            .collect();
        own.sort_by_key(|(_, seq)| *seq);
        own.into_iter().map(|(id, _)| id.clone()).collect()
    }

    /// Closes the notification `id` if it belongs to `extension_id`.
    fn close_own(&self, extension_id: Uuid, id: &str) -> bool {
        let mut open = self.lock();
        if open.get(id).is_none_or(|o| o.extension_id != extension_id) {
            return false;
        }
        let Some(entry) = open.remove(id) else {
            return false;
        };
        drop(open);
        if let Some(shown) = entry.shown {
            shown.close();
        }
        true
    }

    /// Closes every notification of `extension_id` (disabled, removed).
    pub fn close_all(&self, extension_id: Uuid) {
        for id in self.of_extension(extension_id) {
            self.close_own(extension_id, &id);
        }
    }

    /// An open notification of `extension_id` without a system behind it (tests elsewhere).
    #[cfg(test)]
    pub(crate) fn open_for_test(&self, extension_id: Uuid) -> String {
        let id = Uuid::new_v4().to_string();
        self.admit(extension_id, &id, None);
        id
    }

    /// Enters the notification `id` of `extension_id` before it shows, so a response that comes at
    /// once finds it. Under the same lock the one with the same `tag` and the oldest beyond
    /// [`MAX_OPEN`] leave, so calls at the same time cannot pass the limit; they are returned to be
    /// closed outside the lock.
    fn admit(&self, extension_id: Uuid, id: &str, tag: Option<String>) -> Vec<Open> {
        let mut open = self.lock();
        let mut own: Vec<(String, u64)> = open
            .iter()
            .filter(|(_, o)| o.extension_id == extension_id)
            .map(|(id, o)| (id.clone(), o.seq))
            .collect();
        own.sort_by_key(|(_, seq)| *seq);
        let mut gone = Vec::new();
        if let Some(tag) = tag.as_deref() {
            own.retain(|(id, _)| {
                let same = open.get(id).is_some_and(|o| o.tag.as_deref() == Some(tag));
                if same {
                    gone.extend(open.remove(id));
                }
                !same
            });
        }
        let excess = (own.len() + 1).saturating_sub(MAX_OPEN);
        gone.extend(
            own.iter()
                .take(excess)
                .filter_map(|(id, _)| open.remove(id)),
        );
        open.insert(
            id.to_owned(),
            Open {
                extension_id,
                seq: self.next.fetch_add(1, Ordering::Relaxed),
                tag,
                shown: None,
            },
        );
        gone
    }
}

fn invalid(message: impl Into<String>) -> BridgeError {
    BridgeError::new(ExtensionErrorCode::Validation, message)
}

fn optional_text(
    value: Option<&Value>,
    name: &str,
    max: usize,
) -> Result<Option<String>, BridgeError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) if text.chars().count() <= max => Ok(Some(text.clone())),
        Some(_) => Err(invalid(format!(
            "{name} must be a string of at most {max} characters"
        ))),
    }
}

fn required_text(value: Option<&Value>, name: &str, max: usize) -> Result<String, BridgeError> {
    optional_text(value, name, max)?
        .filter(|t| !t.is_empty())
        .ok_or_else(|| invalid(format!("{name} must be a string of 1 to {max} characters")))
}

/// `{path}` of a deep link, if set.
fn link(value: Option<&Value>, name: &str) -> Result<Option<String>, BridgeError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(link) => optional_text(link.get("path"), &format!("{name}.path"), MAX_PATH_CHARS),
    }
}

/// An icon as `data:` URL of an image, or a file of the calling frame's own bundle.
fn icon(
    ctx: &CallContext,
    value: Option<&Value>,
) -> Result<Option<(Vec<u8>, &'static str)>, BridgeError> {
    let Some(value) = optional_text(value, "icon", MAX_ICON_BYTES * 2)? else {
        return Ok(None);
    };
    let too_large = || BridgeError::new(ExtensionErrorCode::LimitExceeded, "icon too large");
    if let Some(data) = value.strip_prefix("data:") {
        let (mime, encoded) = data
            .split_once(";base64,")
            .ok_or_else(|| invalid("icon must be a base64 data: URL or a bundle file"))?;
        let extension = ICON_TYPES
            .iter()
            .find(|(m, _)| m.eq_ignore_ascii_case(mime))
            .map(|(_, e)| *e)
            .ok_or_else(|| invalid("icon must be png, jpeg, webp or gif"))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| invalid("icon must be a base64 data: URL or a bundle file"))?;
        if bytes.len() > MAX_ICON_BYTES {
            return Err(too_large());
        }
        return Ok(Some((bytes, extension)));
    }
    if value.contains("://") {
        return Err(invalid("icon must be a base64 data: URL or a bundle file"));
    }
    let path = value
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_owned();
    let extension = path
        .rsplit_once('.')
        .and_then(|(_, ext)| {
            ICON_TYPES.iter().find(|(_, e)| {
                e.eq_ignore_ascii_case(ext) || (*e == "jpg" && ext.eq_ignore_ascii_case("jpeg"))
            })
        })
        .map(|(_, e)| *e)
        .ok_or_else(|| invalid("icon must be png, jpeg, webp or gif"))?;
    // A development version has no stored bundle; its icon comes as a `data:` URL.
    let bundle_id = ctx
        .session
        .source
        .bundle()
        .ok_or_else(|| invalid("icon file not in the bundle"))?;
    let bytes = ctx
        .db
        .read_blocking(move |q| {
            read_verified_file(q, bundle_id, &path).map_err(haex_crdt::Error::from)
        })
        .map_err(|_| invalid("icon file not readable"))?
        .ok_or_else(|| invalid("icon file not in the bundle"))?;
    if bytes.len() > MAX_ICON_BYTES {
        return Err(too_large());
    }
    Ok(Some((bytes, extension)))
}

/// `{options: {title, body?, icon?, primary?, actions?, tag?}}` of the SDK.
fn parse(
    ctx: &CallContext,
    params: &Value,
) -> Result<(NotificationSpec, Links, Option<String>), BridgeError> {
    let options = params
        .get("options")
        .filter(|o| o.is_object())
        .ok_or_else(|| invalid("options must be an object"))?;
    let mut links = Links {
        primary: link(options.get("primary"), "primary")?,
        ..Links::default()
    };
    let mut buttons = Vec::new();
    if let Some(actions) = options.get("actions").filter(|a| !a.is_null()) {
        let actions = actions
            .as_array()
            .filter(|a| a.len() <= MAX_ACTIONS)
            .ok_or_else(|| invalid(format!("actions must be a list of at most {MAX_ACTIONS}")))?;
        for action in actions {
            let id = required_text(action.get("id"), "actions.id", MAX_ID_CHARS)?;
            let label = required_text(action.get("label"), "actions.label", MAX_LABEL_CHARS)?;
            if links.buttons.contains_key(&id) {
                return Err(invalid("action ids must differ"));
            }
            links.buttons.insert(
                id.clone(),
                link(action.get("deepLink"), "actions.deepLink")?,
            );
            buttons.push(NotificationButton { id, label });
        }
    }
    let spec = NotificationSpec {
        title: required_text(options.get("title"), "title", MAX_TITLE_CHARS)?,
        body: optional_text(options.get("body"), "body", MAX_BODY_CHARS)?,
        icon: icon(ctx, options.get("icon"))?,
        buttons,
    };
    let tag = optional_text(options.get("tag"), "tag", MAX_TAG_CHARS)?;
    Ok((spec, links, tag))
}

/// Allowed, or 1002/1004 with `{resourceType: notifications, action: show, target: *}`.
fn check_permission(ctx: &CallContext) -> Result<(), BridgeError> {
    let (extension_id, device) = (ctx.session.extension_id, ctx.device);
    let mut grants = ctx
        .db
        .read_blocking(move |q| {
            candidates(q, extension_id, PermissionKind::Notifications, device).map_err(Into::into)
        })
        .map_err(|_| BridgeError::new(ExtensionErrorCode::Database, "database unavailable"))?;
    grants.extend(
        ctx.host
            .permissions
            .temporary(extension_id, PermissionKind::Notifications),
    );
    let request = PermissionRequest {
        kind: PermissionKind::Notifications,
        action: Action::Show,
        target: RequestTarget::Any,
    };
    let code = match evaluate(&grants, &request, device) {
        Decision::Allow => return Ok(()),
        Decision::Deny => ExtensionErrorCode::PermissionDenied,
        Decision::Prompt => ExtensionErrorCode::PermissionPromptRequired,
    };
    Err(
        BridgeError::new(code, "permission required").with_details(json!({
            "resourceType": "notifications",
            "action": "show",
            "target": "*",
        })),
    )
}

/// What happens on a response: the notification is forgotten; a click goes to the extension's
/// frames and to holzi's window, which brings its tab and itself forward (only the window that
/// shows the tab comes to the front).
fn respond(
    host: Arc<ExtensionHost>,
    emitter: Arc<dyn Emit>,
    extension_id: Uuid,
    id: String,
    links: Links,
) -> Respond {
    Box::new(move |response| {
        if !host.notifications.forget(&id) {
            return;
        }
        let (action_id, path) = match response {
            NotificationResponse::Closed => return,
            NotificationResponse::Body => (None, links.primary),
            NotificationResponse::Button(button) => {
                let path = links.buttons.get(&button).cloned().flatten();
                (Some(button), path)
            }
        };
        let mut click = json!({ "notificationId": id });
        if let Some(action_id) = action_id {
            click["actionId"] = Value::String(action_id);
        }
        if let Some(path) = &path {
            click["path"] = Value::String(path.clone());
        }
        emit_to_frames(&*emitter, &host, extension_id, CLICK, &click);
        emitter.emit(
            CLICKED,
            json!({ "extensionId": extension_id.to_string(), "path": path }),
        );
    })
}

/// `extension_notifications_show`: `{options}` → `{id}`.
pub fn show(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let (spec, links, tag) = parse(ctx, params)?;
    check_permission(ctx)?;
    let desktop = ctx.host.desktop().ok_or_else(BridgeError::not_available)?;
    let extension_id = ctx.session.extension_id;
    if !desktop.notifications_allowed() {
        // The system shows nothing: the message stays visible inside holzi, where its tag replaces
        // the earlier one. It has no buttons there and is not remembered; a dismiss of its id
        // finds nothing, as for any gone one.
        ctx.emitter.emit(
            IN_APP,
            json!({
                "extensionId": extension_id.to_string(),
                "title": spec.title,
                "body": spec.body,
                "tag": tag,
            }),
        );
        return Ok(json!({ "id": Uuid::new_v4().to_string() }));
    }
    let state = &ctx.host.notifications;
    let id = Uuid::new_v4().to_string();
    for replaced in state.admit(extension_id, &id, tag) {
        if let Some(shown) = replaced.shown {
            shown.close();
        }
    }
    let on_response = respond(
        Arc::clone(&ctx.host),
        Arc::clone(&ctx.emitter),
        extension_id,
        id.clone(),
        links,
    );
    let shown = match desktop.show_notification(&spec, on_response) {
        Ok(shown) => shown,
        Err(_) => {
            state.forget(&id);
            return Err(BridgeError::new(
                ExtensionErrorCode::NotAvailable,
                "notifications unavailable",
            ));
        }
    };
    match state.lock().get_mut(&id) {
        Some(open) => open.shown = Some(shown),
        // Already answered and gone.
        None => shown.close(),
    }
    Ok(json!({ "id": id }))
}

/// `extension_notifications_dismiss`: `{id}`; only an own notification. Another extension's and a
/// missing one get the same answer (FR-062).
pub fn dismiss(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let id = params
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("id must be a string"))?;
    if ctx
        .host
        .notifications
        .close_own(ctx.session.extension_id, id)
    {
        Ok(Value::Null)
    } else {
        Err(BridgeError::new(ExtensionErrorCode::NotFound, "not found"))
    }
}

#[cfg(test)]
#[path = "notifications_tests.rs"]
mod tests;
