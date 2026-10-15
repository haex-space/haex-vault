//! What a caller outside holzi sees of a contact (spec 051 FR-028, contracts/bridge.md): extensions
//! and agents get the same view. Everything a person reads on the contact: the head fields, every
//! value (unknown kinds too), group names, the photo hash and the confirmed holzi addresses. Never
//! where the contact came from, which phones it goes to, what did not reach them or how it is linked
//! there. A field without a value is left out, never `null`.

use serde::Serialize;

use super::model::{Contact, ContactHeader, Group};

/// A contact in a list or a search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutsideSummary {
    pub id: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
    pub phones: Vec<String>,
    pub emails: Vec<String>,
    /// Group names.
    pub groups: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub photo: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutsideValue {
    /// The value's id, which a change names.
    pub id: String,
    pub kind: String,
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutsideDevice {
    pub device_pubkey: String,
    pub endpoint_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iroh_relay: Option<String>,
}

/// The confirmed holzi addresses, from the contact's Visitenkarte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutsideAddresses {
    /// The vault identity, hex.
    pub vault: String,
    pub npub: String,
    pub devices: Vec<OutsideDevice>,
    pub nostr_relays: Vec<String>,
}

/// One contact with everything an outside caller may read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutsideContact {
    pub id: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub given_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub middle_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_suffix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub values: Vec<OutsideValue>,
    /// Group names.
    pub groups: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub photo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holzi_addresses: Option<OutsideAddresses>,
    pub updated_at: String,
}

/// A header as an outside caller sees it; `groups` are all groups, to name the header's group ids.
pub fn summary(header: ContactHeader, groups: &[Group]) -> OutsideSummary {
    OutsideSummary {
        groups: header
            .groups
            .iter()
            .filter_map(|id| groups.iter().find(|group| &group.id == id))
            .map(|group| group.name.clone())
            .collect(),
        id: header.id,
        display_name: header.name,
        organization: header.organization,
        phones: header.phones,
        emails: header.emails,
        photo: header.photo,
    }
}

pub fn contact(contact: Contact) -> OutsideContact {
    let head = contact.head;
    OutsideContact {
        id: contact.id,
        display_name: contact.name,
        given_name: head.given_name,
        family_name: head.family_name,
        middle_name: head.middle_name,
        name_prefix: head.name_prefix,
        name_suffix: head.name_suffix,
        nickname: head.nickname,
        organization: head.organization,
        job_title: head.job_title,
        note: head.note,
        values: contact
            .values
            .into_iter()
            .map(|value| OutsideValue {
                id: value.id,
                kind: value.kind,
                label: value.label,
                value: value.value,
            })
            .collect(),
        groups: contact.groups.into_iter().map(|group| group.name).collect(),
        photo: contact.photo,
        holzi_addresses: contact.card.map(|card| OutsideAddresses {
            vault: card.vault_pubkey,
            npub: card.npub,
            devices: card
                .devices
                .into_iter()
                .map(|device| OutsideDevice {
                    device_pubkey: device.device_pubkey,
                    endpoint_id: device.endpoint_id,
                    iroh_relay: device.iroh_relay,
                })
                .collect(),
            nostr_relays: card.nostr_relays,
        }),
        updated_at: contact.updated_at,
    }
}
