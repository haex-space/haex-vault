//! The address book for extensions (spec 051 US5, FR-028, FR-029, FR-031, contracts/bridge.md): a
//! thin adapter onto the address book's service, like `passwords.rs` onto the password manager's.
//! The extension is the caller (`Caller::Extension`), its `contacts` permissions are its grants,
//! and the rules are the service's. Extensions only read; there is no write method.
//!
//! A permission names all contacts (`*`) or a group by its name; the grants carry group ids, so the
//! names are looked up first. A refusal asks the user only for a permission the extension holds in
//! the state "ask"; without one, and for a contact outside the grants or missing, it is 1002, so an
//! extension never learns whether a contact exists (FR-031).

use std::collections::BTreeSet;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde_json::{json, Value};

use crate::contacts::access::{Caller, Grant, GrantAction, Scope};
use crate::contacts::model::{Contact, ContactHeader, Group};
use crate::contacts::outside;
use crate::contacts::service::ContactsService;
use crate::error::HolziError;
use crate::extensions::bridge::blocking::block_on;
use crate::extensions::bridge::dispatch::CallContext;
use crate::extensions::error::{BridgeError, ExtensionErrorCode};
use crate::extensions::permissions::store::candidates;
use crate::extensions::permissions::{
    Action, Permission, PermissionKind, PermissionStatus, Target,
};
use crate::passwords::ids::fold;

pub const MODULE: &str = module_path!();

/// Contacts of one page of `extension_contacts_list` when the call names no limit, and the most.
const LIST_LIMIT: (usize, usize) = (100, 1000);
/// Hits of `extension_contacts_search` when the call names no limit, and the most.
const SEARCH_LIMIT: (usize, usize) = (20, 200);

/// How holzi's own lookup of the group names is written into the service's checks.
const GROUP_LOOKUP: Caller = Caller::Internal {
    feature: "extension_contacts_grants",
};

fn invalid(message: &str) -> BridgeError {
    BridgeError::new(ExtensionErrorCode::Validation, message)
}

fn unavailable() -> BridgeError {
    BridgeError::new(ExtensionErrorCode::Database, "contacts unavailable")
}

fn denied() -> BridgeError {
    BridgeError::new(ExtensionErrorCode::PermissionDenied, "permission denied")
}

/// The calling extension's `contacts` permissions on this device: remembered and held in memory.
fn permissions(ctx: &CallContext) -> Result<Vec<Permission>, BridgeError> {
    let (extension_id, device) = (ctx.session.extension_id, ctx.device);
    let mut permissions = ctx
        .db
        .read_blocking(move |q| {
            candidates(q, extension_id, PermissionKind::Contacts, device).map_err(Into::into)
        })
        .map_err(|_| unavailable())?;
    permissions.extend(
        ctx.host
            .permissions
            .temporary(extension_id, PermissionKind::Contacts),
    );
    Ok(permissions)
}

fn denies_all(permissions: &[Permission]) -> bool {
    permissions
        .iter()
        .any(|p| p.status == PermissionStatus::Denied && p.target == Target::Any)
}

/// The (folded) group names of the denied permissions.
fn denied_names(permissions: &[Permission]) -> BTreeSet<&str> {
    permissions
        .iter()
        .filter(|p| p.status == PermissionStatus::Denied)
        .filter_map(|p| match &p.target {
            Target::Tag(name) => Some(name.as_str()),
            _ => None,
        })
        .collect()
}

/// The ids of the groups with this folded name; names need not be unique.
fn ids_named(groups: &[Group], name: &str) -> BTreeSet<String> {
    groups
        .iter()
        .filter(|group| fold(&group.name) == name)
        .map(|group| group.id.clone())
        .collect()
}

/// The grants of the service: every granted `read`. A denied `*` takes them all away; a denied
/// group hides its members from a `*` grant and voids a grant for the same group, but a contact
/// also in another granted group stays visible through that one (spec 017 FR-017).
pub fn grants_of(permissions: &[Permission], groups: &[Group]) -> Vec<Grant> {
    if denies_all(permissions) {
        return Vec::new();
    }
    let denied = denied_names(permissions);
    let denied_ids: BTreeSet<String> = denied
        .iter()
        .flat_map(|name| ids_named(groups, name))
        .collect();
    permissions
        .iter()
        .filter(|p| p.status == PermissionStatus::Granted && p.action == Action::Read)
        .filter_map(|p| {
            let scope = match &p.target {
                Target::Any => Scope::all_except(denied_ids.clone()),
                Target::Tag(name) if denied.contains(name.as_str()) => return None,
                Target::Tag(name) => Scope::Groups(ids_named(groups, name)),
                _ => return None,
            };
            Some(Grant::new(GrantAction::Read, scope))
        })
        .collect()
}

/// The answer to a call without grants: a question for a permission in the state "ask" (the one
/// for `group` first, when the call names one), else 1002. Under a denied `*`, or for a permission
/// on a denied group, no answer could help, so none is asked. The question names a group as the
/// address book spells it, not in its folded form.
fn refused(permissions: &[Permission], groups: &[Group], group: Option<&str>) -> BridgeError {
    if denies_all(permissions) {
        return denied();
    }
    let denied_groups = denied_names(permissions);
    let asking: Vec<&Permission> = permissions
        .iter()
        .filter(|p| p.status == PermissionStatus::Ask && p.action == Action::Read)
        .filter(
            |p| !matches!(&p.target, Target::Tag(name) if denied_groups.contains(name.as_str())),
        )
        .collect();
    let names = |p: &Permission| match (&p.target, group) {
        (Target::Any, _) => true,
        (Target::Tag(name), Some(group)) => *name == fold(group),
        _ => false,
    };
    let chosen = asking
        .iter()
        .copied()
        .find(|p| names(p))
        .or_else(|| asking.first().copied().filter(|_| group.is_none()));
    let target = match chosen.map(|p| &p.target) {
        Some(Target::Any) => "*".to_owned(),
        Some(Target::Tag(name)) => groups
            .iter()
            .find(|group| fold(&group.name) == *name)
            .map_or_else(|| name.clone(), |group| group.name.clone()),
        _ => return denied(),
    };
    BridgeError::new(
        ExtensionErrorCode::PermissionPromptRequired,
        "permission required",
    )
    .with_details(json!({
        "resourceType": "contacts",
        "action": "read",
        "target": target,
    }))
}

/// Maps a failure of the service; a contact outside the grants and a missing one look the same.
fn map_error(error: HolziError, c: &Call, group: Option<&str>) -> BridgeError {
    match error {
        HolziError::ContactsForbidden => refused(&c.permissions, &c.groups, group),
        HolziError::ContactsNotFound => denied(),
        HolziError::ContactsInvalid { reason } => invalid(&reason),
        _ => unavailable(),
    }
}

/// What every call needs: the service, the caller, its permissions and the groups by id.
struct Call {
    service: ContactsService,
    caller: Caller,
    permissions: Vec<Permission>,
    grants: Vec<Grant>,
    groups: Vec<Group>,
}

impl Call {
    /// The SDK's `ContactSummary`.
    fn summary(&self, header: ContactHeader) -> Value {
        json!(outside::summary(header, &self.groups))
    }
}

fn call(ctx: &CallContext) -> Result<Call, BridgeError> {
    let service = ContactsService::new(ctx.db.clone());
    let permissions = permissions(ctx)?;
    let groups = block_on(service.groups(&GROUP_LOOKUP)).map_err(|_| unavailable())?;
    Ok(Call {
        caller: Caller::Extension {
            id: ctx.session.extension_id.to_string(),
        },
        grants: grants_of(&permissions, &groups),
        service,
        permissions,
        groups,
    })
}

/// The SDK's `ContactDetail` (`contacts::outside`).
fn detail(contact: Contact) -> Value {
    json!(outside::contact(contact))
}

fn text<'a>(params: &'a Value, key: &str) -> Result<Option<&'a str>, BridgeError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text)),
        Some(_) => Err(invalid(&format!("{key} must be a string"))),
    }
}

fn required(params: &Value, key: &str) -> Result<String, BridgeError> {
    text(params, key)?
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| invalid(&format!("{key} must be a string")))
}

/// A count of the call: absent is `default`, more than `most` is `most`.
fn count(params: &Value, key: &str, (default, most): (usize, usize)) -> Result<usize, BridgeError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(value) => value
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .map(|n| n.min(most))
            .ok_or_else(|| invalid(&format!("{key} must be a whole number"))),
    }
}

/// `extension_contacts_list {group?, offset?, limit?}` → `{contacts, total}`: the contacts in scope,
/// those of the named group only when one is named.
pub fn list(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let group = text(params, "group")?;
    let offset = count(params, "offset", (0, usize::MAX))?;
    let limit = count(params, "limit", LIST_LIMIT)?;
    let c = call(ctx)?;
    let mut headers =
        block_on(c.service.list(&c.caller, &c.grants)).map_err(|e| map_error(e, &c, group))?;
    if let Some(group) = group {
        let ids = ids_named(&c.groups, &fold(group));
        headers.retain(|header| header.groups.iter().any(|id| ids.contains(id)));
    }
    let total = headers.len();
    let contacts: Vec<Value> = headers
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|header| c.summary(header))
        .collect();
    Ok(json!({ "contacts": contacts, "total": total }))
}

/// `extension_contacts_read {id}` → `ContactDetail`.
pub fn read(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let id = required(params, "id")?;
    let c = call(ctx)?;
    let contact =
        block_on(c.service.get(&c.caller, &c.grants, &id)).map_err(|e| map_error(e, &c, None))?;
    Ok(detail(contact))
}

/// `extension_contacts_search {query, limit?}` → `{contacts}`: the window's search over the
/// contacts in scope.
pub fn search(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let query = text(params, "query")?.ok_or_else(|| invalid("query must be a string"))?;
    let limit = count(params, "limit", SEARCH_LIMIT)?;
    let c = call(ctx)?;
    let headers = block_on(c.service.search(&c.caller, &c.grants, query))
        .map_err(|e| map_error(e, &c, None))?;
    let contacts: Vec<Value> = headers
        .into_iter()
        .take(limit)
        .map(|header| c.summary(header))
        .collect();
    Ok(json!({ "contacts": contacts }))
}

/// `extension_contacts_photo {hash}` → the JPEG as Base64, for a photo a readable contact uses.
pub fn photo(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let hash = required(params, "hash")?;
    let c = call(ctx)?;
    block_on(c.service.photo(&c.caller, &c.grants, &hash))
        .map(|bytes| Value::String(STANDARD.encode(bytes)))
        .map_err(|e| map_error(e, &c, None))
}

#[cfg(test)]
#[path = "contacts_tests.rs"]
mod tests;
