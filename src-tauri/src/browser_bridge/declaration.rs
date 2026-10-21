//! The declaration of a client (spec 049, FR-006, FR-010, FR-012, research R5): what it says it
//! needs in the handshake, compared in a canonical form, and the widest grant it allows. Only
//! `permissions.core.passwords` counts; every other category is kept for comparison and never
//! granted.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

/// The kind of a grant for the password manager (034 FR-025).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum BridgeAccess {
    Read,
    ReadWrite,
}

impl BridgeAccess {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::ReadWrite => "readWrite",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "read" => Some(Self::Read),
            "readWrite" => Some(Self::ReadWrite),
            _ => None,
        }
    }
}

/// A handshake without password permissions (FR-006: `PERMISSIONS_DECLARATION_REQUIRED`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeclarationRequired;

/// One declared entry: a kind and a target (`*` or a tag name).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    access: BridgeAccess,
    target: String,
}

/// The password part of a declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    entries: Vec<Entry>,
}

impl Declared {
    /// Reads `permissions.core.passwords`; a missing or empty list is no declaration.
    pub fn read(permissions: Option<&Value>) -> Result<Self, DeclarationRequired> {
        let list = permissions
            .and_then(|p| p.get("core"))
            .and_then(|c| c.get("passwords"))
            .and_then(Value::as_array)
            .ok_or(DeclarationRequired)?;
        let entries: Vec<Entry> = list
            .iter()
            .filter_map(|item| {
                let target = item.get("target")?.as_str()?.trim();
                if target.is_empty() {
                    return None;
                }
                let access = match item.get("operation").and_then(Value::as_str) {
                    None => BridgeAccess::Read,
                    Some(op) => BridgeAccess::parse(op)?,
                };
                Some(Entry {
                    access,
                    target: target.to_owned(),
                })
            })
            .collect();
        if entries.is_empty() {
            return Err(DeclarationRequired);
        }
        Ok(Self { entries })
    }

    /// The targets declared for at least `access`; `None` stands for every entry (`*`).
    fn targets_for(&self, access: BridgeAccess) -> Option<BTreeSet<String>> {
        let mut tags = BTreeSet::new();
        for entry in self.entries.iter().filter(|e| e.access >= access) {
            if entry.target == "*" {
                return None;
            }
            tags.insert(entry.target.clone());
        }
        Some(tags)
    }

    fn declares(&self, access: BridgeAccess) -> bool {
        self.entries.iter().any(|e| e.access >= access)
    }

    /// Whether writing is declared at all (FR-015: no prompt without it).
    pub fn declares_write(&self) -> bool {
        self.declares(BridgeAccess::ReadWrite)
    }

    /// The grant the dialog proposes: as declared, the widest kind first (FR-010).
    pub fn proposal(&self) -> (BridgeAccess, Option<Vec<String>>) {
        let access = if self.declares_write() {
            BridgeAccess::ReadWrite
        } else {
            BridgeAccess::Read
        };
        let scope = self
            .targets_for(access)
            .map(|tags| tags.into_iter().collect());
        (access, scope)
    }

    /// Whether a grant stays within the declaration: never wider, only narrower (FR-010).
    /// `scope_tags` `None` means every entry.
    pub fn fits(&self, access: BridgeAccess, scope_tags: Option<&[String]>) -> bool {
        if !self.declares(access) {
            return false;
        }
        match (self.targets_for(access), scope_tags) {
            (None, _) => true,
            (Some(_), None) => false,
            (Some(declared), Some(chosen)) => {
                !chosen.is_empty() && chosen.iter().all(|tag| declared.contains(tag))
            }
        }
    }
}

/// The declaration in a form that does not depend on the order of keys or lists, for storing and
/// comparing (FR-012).
pub fn canonical(permissions: Option<&Value>, requested_extensions: Option<&Value>) -> String {
    let mut root = Map::new();
    root.insert(
        "permissions".into(),
        sorted(permissions.cloned().unwrap_or(Value::Null)),
    );
    root.insert(
        "requestedExtensions".into(),
        sorted(requested_extensions.cloned().unwrap_or(Value::Null)),
    );
    sorted(Value::Object(root)).to_string()
}

fn sorted(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<(String, Value)> = map.into_iter().collect();
            keys.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(keys.into_iter().map(|(k, v)| (k, sorted(v))).collect())
        }
        Value::Array(items) => {
            let mut items: Vec<Value> = items.into_iter().map(sorted).collect();
            items.sort_by_key(Value::to_string);
            Value::Array(items)
        }
        other => other,
    }
}
