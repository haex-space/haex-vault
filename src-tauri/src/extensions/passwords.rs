//! The password functions of extensions (spec 017, US10, T105, FR-053, research R21): a thin
//! adapter onto the password manager's service (spec 034, `contracts/access.md`). The extension is
//! the caller (`Caller::Extension`), its `passwords` permissions are its grants, and every rule
//! Z1–Z13 is the service's, not this module's.
//!
//! A refusal asks the user only for a permission the extension holds in the state "ask" (declared
//! in its manifest and not ticked at install, or remembered so); without one it is 1002 (Z3).

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::HolziError;
use crate::extensions::bridge::blocking::block_on;
use crate::extensions::bridge::dispatch::CallContext;
use crate::extensions::error::{BridgeError, ExtensionErrorCode};
use crate::extensions::permissions::store::candidates;
use crate::extensions::permissions::{
    Action, Permission, PermissionKind, PermissionStatus, Target,
};
use crate::passwords::access::{Caller, Grant, GrantAction, Scope};
use crate::passwords::ids::fold;
use crate::passwords::kind::Kinds;
use crate::passwords::model::{
    ItemHeader, ItemInput, ItemPatch, KeyValueInput, KeyValuePatch, Patch, SecretItem,
    SecretKeyValue,
};
use crate::passwords::service::{Headers, PasswordsService};

pub const MODULE: &str = module_path!();

fn invalid(message: &str) -> BridgeError {
    BridgeError::new(ExtensionErrorCode::Validation, message)
}

fn unavailable() -> BridgeError {
    BridgeError::new(ExtensionErrorCode::Database, "passwords unavailable")
}

/// The stored text of a `passwords` target.
fn target_text(target: &Target) -> Option<String> {
    match target {
        Target::Any => Some("*".to_owned()),
        Target::Tag(tag) => Some(tag.clone()),
        _ => None,
    }
}

/// The calling extension's `passwords` permissions on this device: remembered and held in memory.
fn permissions(ctx: &CallContext) -> Result<Vec<Permission>, BridgeError> {
    let (extension_id, device) = (ctx.session.extension_id, ctx.device);
    let mut permissions = ctx
        .db
        .read_blocking(move |q| {
            candidates(q, extension_id, PermissionKind::Passwords, device).map_err(Into::into)
        })
        .map_err(|_| unavailable())?;
    permissions.extend(
        ctx.host
            .permissions
            .temporary(extension_id, PermissionKind::Passwords),
    );
    Ok(permissions)
}

fn denies_all(permissions: &[Permission]) -> bool {
    permissions
        .iter()
        .any(|p| p.status == PermissionStatus::Denied && p.target == Target::Any)
}

/// The (folded) tags of the denied permissions, whatever their action.
fn denied_tags(permissions: &[Permission]) -> BTreeSet<&str> {
    permissions
        .iter()
        .filter(|p| p.status == PermissionStatus::Denied)
        .filter_map(|p| match &p.target {
            Target::Tag(tag) => Some(tag.as_str()),
            _ => None,
        })
        .collect()
}

/// The grants of the service: every granted permission. A denied `*` takes them all away. A denied
/// tag hides the entries carrying it from a `*` grant and voids a grant for the same tag (FR-017),
/// but an entry that also carries another granted tag stays visible through that one.
pub fn grants_of(permissions: &[Permission]) -> Vec<Grant> {
    if denies_all(permissions) {
        return Vec::new();
    }
    let denied = denied_tags(permissions);
    permissions
        .iter()
        .filter(|p| p.status == PermissionStatus::Granted)
        .filter_map(|p| {
            let action = match p.action {
                Action::Read => GrantAction::Read,
                Action::ReadWrite => GrantAction::ReadWrite,
                _ => return None,
            };
            let scope = match &p.target {
                Target::Any => Scope::all_except(&denied),
                Target::Tag(tag) if denied.contains(tag.as_str()) => return None,
                Target::Tag(tag) => Scope::tags([tag]),
                _ => return None,
            };
            Some(Grant::new(action, scope))
        })
        .collect()
}

/// The answer to a refusal of the service: a question for a permission in the state "ask" that
/// would cover the call (one of `tags` first, for a write that names them), else 1002. No answer
/// could help under a denied `*`, for a call that `adds` a denied tag to an entry, or for a
/// permission on a denied tag (`grants_of` voids it), so none of these is asked.
fn refused(
    permissions: &[Permission],
    write: bool,
    tags: &[String],
    adds: &[String],
) -> BridgeError {
    let denied = denied_tags(permissions);
    if denies_all(permissions) || adds.iter().any(|tag| denied.contains(fold(tag).as_str())) {
        return BridgeError::new(ExtensionErrorCode::PermissionDenied, "permission denied");
    }
    let needed = if write {
        Action::ReadWrite
    } else {
        Action::Read
    };
    let asking: Vec<&Permission> = permissions
        .iter()
        .filter(|p| p.status == PermissionStatus::Ask && p.action.covers(&needed))
        .filter(|p| !matches!(&p.target, Target::Tag(tag) if denied.contains(tag.as_str())))
        .collect();
    let names = |p: &Permission| match &p.target {
        Target::Any => true,
        Target::Tag(tag) => tags.iter().any(|t| fold(t) == *tag),
        _ => false,
    };
    let chosen = asking
        .iter()
        .copied()
        .find(|p| names(p))
        .or_else(|| asking.first().copied().filter(|_| tags.is_empty()));
    match chosen.and_then(|p| target_text(&p.target)) {
        Some(target) => BridgeError::new(
            ExtensionErrorCode::PermissionPromptRequired,
            "permission required",
        )
        .with_details(json!({
            "resourceType": "passwords",
            "action": if write { "readWrite" } else { "read" },
            "target": target,
        })),
        None => BridgeError::new(ExtensionErrorCode::PermissionDenied, "permission denied"),
    }
}

/// Maps a failure of the service; no message carries a value of the entry (Z10). `tags` are the
/// submitted tags, `adds` those of them the entry does not carry yet.
fn map_error(
    error: HolziError,
    permissions: &[Permission],
    write: bool,
    tags: &[String],
    adds: &[String],
) -> BridgeError {
    match error {
        HolziError::PasswordsForbidden => refused(permissions, write, tags, adds),
        HolziError::PasswordsNotFound => {
            BridgeError::new(ExtensionErrorCode::NotFound, "not found")
        }
        HolziError::InvalidInput { reason }
        | HolziError::PasswordsConflict { reason }
        | HolziError::PasswordsReference { reason } => {
            BridgeError::new(ExtensionErrorCode::Validation, reason)
        }
        HolziError::PasswordsReferenceCycle { .. } => invalid("reference cycle"),
        _ => unavailable(),
    }
}

/// The SDK's `PasswordInput`.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct SdkInput {
    title: Option<String>,
    username: Option<String>,
    password: Option<String>,
    note: Option<String>,
    icon: Option<String>,
    color: Option<String>,
    url: Option<String>,
    otp_secret: Option<String>,
    otp_digits: Option<u32>,
    otp_period: Option<u32>,
    otp_algorithm: Option<String>,
    autofill_aliases: Option<Aliases>,
    expires_at: Option<String>,
    tags: Vec<String>,
    key_values: Option<Vec<SdkKeyValue>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct SdkKeyValue {
    key: Option<String>,
    value: Option<String>,
}

fn input_of(params: &Value) -> Result<SdkInput, BridgeError> {
    let input = params
        .get("input")
        .filter(|i| i.is_object())
        .ok_or_else(|| invalid("input must be an object"))?;
    serde_json::from_value(input.clone()).map_err(|_| invalid("input not understood"))
}

fn item_id(params: &Value) -> Result<String, BridgeError> {
    params
        .get("itemId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| invalid("itemId must be a string"))
}

/// The autofill aliases; ordered, so the stored text does not change with the order of a map.
type Aliases = BTreeMap<String, Vec<String>>;

fn aliases_text(aliases: Option<Aliases>) -> Option<String> {
    aliases.and_then(|a| serde_json::to_string(&a).ok())
}

/// The stored aliases as the SDK's map; a text of another shape reads as none.
fn aliases_of(text: Option<&str>) -> Option<Aliases> {
    text.and_then(|text| serde_json::from_str(text).ok())
}

/// A field of the whole entry the SDK sends, against what the extension read: the same value is
/// `Keep`, so a placeholder stays a placeholder and a field the extension could not see stays as
/// it is (spec 036, FR-047); anything else is set or, when left out, cleared.
fn against<T: PartialEq>(sent: Option<T>, seen: &Option<T>) -> Patch<T> {
    if sent == *seen {
        Patch::Keep
    } else {
        sent.map_or(Patch::Clear, Patch::Set)
    }
}

impl SdkInput {
    fn into_create(self) -> ItemInput {
        ItemInput {
            title: self.title,
            username: self.username,
            password: self.password,
            note: self.note,
            url: self.url,
            icon: self.icon,
            color: self.color,
            expires_at: self.expires_at,
            otp_secret: self.otp_secret,
            otp_digits: self.otp_digits,
            otp_period: self.otp_period,
            otp_algorithm: self.otp_algorithm,
            autofill_aliases: aliases_text(self.autofill_aliases),
            tags: self.tags,
            key_values: self
                .key_values
                .unwrap_or_default()
                .into_iter()
                .map(|kv| KeyValueInput {
                    key: kv.key.unwrap_or_default(),
                    value: kv.value,
                })
                .collect(),
        }
    }

    /// The SDK sends the whole entry, so the patch is its difference to `seen`, the entry as the
    /// extension read it: a field left out is cleared, the tags and custom fields are replaced.
    fn into_patch(self, seen: &SecretItem) -> ItemPatch {
        fn otp<T>(same: bool, sent: Option<T>) -> Patch<T> {
            if same {
                Patch::Keep
            } else {
                sent.map_or(Patch::Clear, Patch::Set)
            }
        }
        // The TOTP parts go together: a new secret without its parts would reset them.
        let otp_same = self.otp_secret == seen.otp_secret
            && self.otp_digits == seen.otp_digits
            && self.otp_period == seen.otp_period
            && self.otp_algorithm == seen.otp_algorithm;
        ItemPatch {
            title: against(self.title, &seen.title),
            username: against(self.username, &seen.username),
            password: against(self.password, &seen.password),
            note: against(self.note, &seen.note),
            url: against(self.url, &seen.url),
            icon: against(self.icon, &seen.icon),
            color: against(self.color, &seen.color),
            expires_at: against(self.expires_at, &seen.expires_at),
            otp_secret: otp(otp_same, self.otp_secret),
            otp_digits: otp(otp_same, self.otp_digits),
            otp_period: otp(otp_same, self.otp_period),
            otp_algorithm: otp(otp_same, self.otp_algorithm),
            autofill_aliases: against(
                aliases_text(self.autofill_aliases),
                &aliases_text(aliases_of(seen.autofill_aliases.as_deref())),
            ),
            tags: (self.tags != seen.tags).then_some(self.tags),
            key_values: key_values_against(self.key_values.unwrap_or_default(), &seen.key_values),
        }
    }
}

/// The custom fields the SDK sends, against those the extension read. They carry no id, so a sent
/// field takes the row of the first unclaimed read field with its key: an unchanged value keeps
/// the stored one (a placeholder stays), the row keeps its id. `None` when nothing changed; the
/// service keeps the fields the extension cannot see.
fn key_values_against(
    sent: Vec<SdkKeyValue>,
    seen: &[SecretKeyValue],
) -> Option<Vec<KeyValuePatch>> {
    let unchanged = sent.len() == seen.len()
        && sent
            .iter()
            .zip(seen)
            .all(|(s, r)| s.key == r.key && s.value == r.value);
    if unchanged {
        return None;
    }
    let mut unclaimed: Vec<&SecretKeyValue> = seen.iter().collect();
    Some(
        sent.into_iter()
            .map(|kv| {
                let read = unclaimed
                    .iter()
                    .position(|r| r.key == kv.key)
                    .map(|at| unclaimed.remove(at));
                let key = kv.key.unwrap_or_default();
                match read {
                    Some(read) => KeyValuePatch {
                        id: Some(read.id.clone()),
                        key,
                        value: (kv.value != read.value).then(|| kv.value.unwrap_or_default()),
                    },
                    None => KeyValuePatch {
                        id: None,
                        key,
                        value: kv.value,
                    },
                }
            })
            .collect(),
    )
}

/// The SDK's `PasswordItemSummary`.
fn summary(header: ItemHeader) -> Value {
    json!({
        "id": header.id,
        "title": header.title,
        "username": header.username,
        "url": header.url,
        "icon": header.icon,
        "color": header.color,
        "tags": header.tags.into_iter().map(|t| t.name).collect::<Vec<_>>(),
        "createdAt": header.created_at,
        "updatedAt": header.updated_at,
        // Spec 092: `login` or `pass`; the list asks for logins unless the extension says otherwise.
        "type": header.kind.to_string(),
    })
}

/// The SDK's `PasswordItemFull`.
fn full(item: SecretItem) -> Value {
    let aliases = aliases_of(item.autofill_aliases.as_deref());
    json!({
        "id": item.id,
        "title": item.title,
        "username": item.username,
        "password": item.password,
        "note": item.note,
        "icon": item.icon,
        "color": item.color,
        "url": item.url,
        "otpSecret": item.otp_secret,
        "otpDigits": item.otp_digits,
        "otpPeriod": item.otp_period,
        "otpAlgorithm": item.otp_algorithm,
        "autofillAliases": aliases,
        "tags": item.tags,
        "keyValues": item.key_values.into_iter().map(|kv| json!({
            "id": kv.id,
            "key": kv.key,
            "value": kv.value,
        })).collect::<Vec<_>>(),
        "expiresAt": item.expires_at,
        "createdAt": item.created_at,
        "updatedAt": item.updated_at,
    })
}

/// What every call needs: the service, the caller and its permissions.
struct Call {
    service: PasswordsService,
    caller: Caller,
    permissions: Vec<Permission>,
    grants: Vec<Grant>,
}

fn call(ctx: &CallContext) -> Result<Call, BridgeError> {
    let permissions = permissions(ctx)?;
    Ok(Call {
        service: PasswordsService::new(ctx.db.clone()),
        caller: Caller::Extension {
            id: ctx.session.extension_id.to_string(),
        },
        grants: grants_of(&permissions),
        permissions,
    })
}

/// `extension_password_list {type?}`: the entries in scope, without secrets (Z4). `type` is
/// `login` (the default), `pass` or `any` (spec 092, FR-027); a pass cannot be read, changed or
/// deleted from an extension, only listed.
pub fn list(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let kinds = match params.get("type").and_then(Value::as_str) {
        None | Some("login") => Kinds::Logins,
        Some("pass") => Kinds::Passes,
        Some("any") => Kinds::Any,
        Some(_) => return Err(invalid("type is login, pass or any")),
    };
    let c = call(ctx)?;
    let headers = block_on(c.service.list_headers_of(&c.caller, &c.grants, kinds))
        .map_err(|e| map_error(e, &c.permissions, false, &[], &[]))?;
    match headers {
        Headers::Items(items) => Ok(Value::Array(items.into_iter().map(summary).collect())),
        Headers::Agent(_) => Err(unavailable()),
    }
}

/// `extension_password_read {itemId}`: one entry with its secrets (Z5, Z13).
pub fn read(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let id = item_id(params)?;
    let c = call(ctx)?;
    block_on(c.service.read_secret_item(&c.caller, &c.grants, id))
        .map(full)
        .map_err(|e| map_error(e, &c.permissions, false, &[], &[]))
}

/// `extension_password_create {input}` → the new id (Z6).
pub fn create(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let input = input_of(params)?.into_create();
    let tags = input.tags.clone();
    let c = call(ctx)?;
    block_on(c.service.create_item(&c.caller, &c.grants, input, None))
        .map(Value::String)
        .map_err(|e| map_error(e, &c.permissions, true, &tags, &tags))
}

/// `extension_password_update {itemId, input}`: the SDK has no version token, so the entry is read
/// first, through the same checks (Z7, Z12); its token guards the write, and the patch is the
/// difference to what was read.
pub fn update(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let id = item_id(params)?;
    let input = input_of(params)?;
    let tags = input.tags.clone();
    let c = call(ctx)?;
    let seen = block_on(c.service.read_secret_item(&c.caller, &c.grants, id.clone()))
        .map_err(|e| map_error(e, &c.permissions, true, &tags, &[]))?;
    let adds: Vec<String> = tags
        .iter()
        .filter(|tag| !seen.tags.iter().any(|carried| fold(carried) == fold(tag)))
        .cloned()
        .collect();
    let patch = input.into_patch(&seen);
    let token = seen.updated_at.unwrap_or_default();
    block_on(
        c.service
            .update_item(&c.caller, &c.grants, id, token, patch),
    )
    .map(|_| Value::Null)
    .map_err(|e| map_error(e, &c.permissions, true, &tags, &adds))
}

/// `extension_password_delete {itemId}`: into the trash (Z8).
pub fn delete(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let id = item_id(params)?;
    let c = call(ctx)?;
    block_on(c.service.delete_item(&c.caller, &c.grants, id))
        .map(|()| Value::Null)
        .map_err(|e| map_error(e, &c.permissions, true, &[], &[]))
}

#[cfg(test)]
#[path = "passwords_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "passwords_denied_tests.rs"]
mod denied_tests;

#[cfg(test)]
#[path = "passwords_owner_tests.rs"]
mod owner_tests;
