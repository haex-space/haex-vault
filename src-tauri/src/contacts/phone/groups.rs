//! Groups between holzi and the phone (spec 051 T096). A membership travels as the group's title:
//! the plugin names the group of each `GroupMembership` row it reads, and a membership holzi
//! writes carries the title until the run puts the id of the group with that title in the raw
//! contact's account in its place. Groups an account lacks are made first, in a call of their
//! own; the system's own groups ("My Contacts", "Starred") are never written to.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value as Json};
use tauri_plugin_holzi_android::contacts::{Op, Target};

use super::gather::Gathered;
use super::write::{Batch, Batches, MAX_OPS};
use super::PhoneBook;

const MEMBERSHIP: &str = "GroupMembership";

type Account = (String, String);

/// The raw contact and title of a membership still waiting for its group's id.
fn titled(op: &Op) -> Option<(&Target, &str)> {
    match op {
        Op::InsertData {
            raw,
            mimetype,
            fields,
        } if mimetype == MEMBERSHIP => Some((raw, fields.get("title")?.as_str()?)),
        _ => None,
    }
}

/// The account of the raw contact an op writes to: a linked one's, or the one a new raw contact
/// of the same call goes into.
fn account_of(call: &Batch, raw: &Target, gathered: &Gathered) -> Option<Account> {
    match raw {
        Target::Id(id) => gathered
            .index
            .get(id)
            .map(|raw| (raw.account_type.clone(), raw.account_name.clone())),
        Target::Ref(reference) => call.ops.iter().find_map(|op| match op {
            Op::InsertRaw {
                account_type,
                account_name,
                reference: other,
            } if other == reference => Some((account_type.clone(), account_name.clone())),
            _ => None,
        }),
    }
}

/// The groups of an account holzi may write to, by title; empty when the phone does not answer.
fn writable(book: &dyn PhoneBook, account: &Account) -> BTreeMap<String, i64> {
    match book.groups(&account.0, &account.1) {
        Ok(groups) => groups
            .into_iter()
            .rev()
            .filter(|group| !group.read_only)
            .map(|group| (group.title, group.id))
            .collect(),
        Err(error) => {
            log::warn!("contacts: the phone's groups were not read: {error}");
            BTreeMap::new()
        }
    }
}

/// Puts the ids of the phone's groups into the memberships of the batches, making the groups an
/// account lacks. A membership whose group the phone does not make is left out; the run goes on.
pub fn resolve(book: &dyn PhoneBook, batches: &mut Batches, gathered: &Gathered) {
    let mut wanted: BTreeMap<Account, BTreeSet<String>> = BTreeMap::new();
    for call in &batches.calls {
        for (raw, title) in call.ops.iter().filter_map(titled) {
            if let Some(account) = account_of(call, raw, gathered) {
                wanted.entry(account).or_default().insert(title.to_owned());
            }
        }
    }
    if wanted.is_empty() {
        return;
    }
    let mut known: BTreeMap<Account, BTreeMap<String, i64>> = BTreeMap::new();
    let mut missing = Vec::new();
    for (account, titles) in &wanted {
        let groups = writable(book, account);
        for title in titles.iter().filter(|title| !groups.contains_key(*title)) {
            missing.push(Op::InsertGroup {
                account_type: account.0.clone(),
                account_name: account.1.clone(),
                title: title.clone(),
            });
        }
        known.insert(account.clone(), groups);
    }
    if !missing.is_empty() {
        for chunk in missing.chunks(MAX_OPS) {
            match book.apply(chunk) {
                Ok(result) if result.error.is_none() => {}
                Ok(result) => log::warn!(
                    "contacts: the phone refused new groups: {}",
                    result.error.unwrap_or_default()
                ),
                Err(error) => log::warn!("contacts: the phone refused new groups: {error}"),
            }
        }
        for (account, groups) in &mut known {
            *groups = writable(book, account);
        }
    }
    for call in &mut batches.calls {
        let ids: Vec<Option<Option<i64>>> = call
            .ops
            .iter()
            .map(|op| {
                let (raw, title) = titled(op)?;
                let account = account_of(call, raw, gathered);
                Some(account.and_then(|account| known.get(&account)?.get(title).copied()))
            })
            .collect();
        let ops = std::mem::take(&mut call.ops);
        let purposes = std::mem::take(&mut call.purposes);
        for ((op, purpose), id) in ops.into_iter().zip(purposes).zip(ids) {
            let op = match (op, id) {
                (op, None) => op,
                (Op::InsertData { raw, mimetype, .. }, Some(Some(group))) => {
                    let mut fields = Map::new();
                    fields.insert("groupId".to_owned(), Json::String(group.to_string()));
                    Op::InsertData {
                        raw,
                        mimetype,
                        fields,
                    }
                }
                (_, Some(_)) => continue,
            };
            call.ops.push(op);
            call.purposes.push(purpose);
        }
    }
}

#[cfg(test)]
#[path = "groups_tests.rs"]
mod tests;
