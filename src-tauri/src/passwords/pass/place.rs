//! Where a new pass from a file or a scan is proposed to go (spec 092, FR-019, amended 2026-11-12):
//! a ticket-like pass (a ticket or a boarding pass) with a date is proposed the tag `Ticket`, and a
//! folder only when the user has made one (`Tickets`, or `Tickets/<provider>` when that exists
//! too); every other pass is proposed for the top of Wallet with no tag. The proposal creates
//! nothing: [`resolve_path`] creates a missing folder when the user confirms, and reuses an
//! existing one of the same name with the case ignored. No rule moves an entry that exists.

use haex_crdt::CrdtTransaction;
use serde::Serialize;
use ts_rs::TS;

use super::model::PassKind;
use crate::error::Result;
use crate::passwords::{groups, WALLET_GROUP_ID};
use crate::storage::query::Query;

/// The folder a user may have made for tickets.
const TICKETS_FOLDER: &str = "Tickets";
/// The tag proposed for a ticket with a date.
pub const TICKET_TAG: &str = "Ticket";

/// A proposed place: the names of the folders below the root of Wallet, outermost first, and a tag.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    pub folder_path: Option<Vec<String>>,
    pub tag_name: Option<String>,
}

/// Whether a kind of pass is ticket-like (FR-019).
pub fn ticket_like(kind: &PassKind) -> bool {
    matches!(kind, PassKind::Ticket | PassKind::BoardingPass)
}

/// The proposal for a pass of this kind. `provider` names the issuer, `has_date` says whether the
/// pass carries a start.
pub fn proposal(
    q: &mut impl Query,
    kind: &PassKind,
    has_date: bool,
    provider: Option<&str>,
) -> Result<Proposal> {
    if !(ticket_like(kind) && has_date) {
        return Ok(Proposal::default());
    }
    let mut proposal = Proposal {
        folder_path: None,
        tag_name: Some(TICKET_TAG.to_owned()),
    };
    let top = groups::siblings(q, Some(WALLET_GROUP_ID))?;
    let Some(tickets) = top
        .iter()
        .find(|g| named(g.name.as_deref(), TICKETS_FOLDER))
    else {
        return Ok(proposal);
    };
    let mut path = vec![tickets.name.clone().unwrap_or_default()];
    if let Some(provider) = provider.map(str::trim).filter(|p| !p.is_empty()) {
        let below = groups::siblings(q, Some(&tickets.id))?;
        if let Some(own) = below.iter().find(|g| named(g.name.as_deref(), provider)) {
            path.push(own.name.clone().unwrap_or_default());
        }
    }
    proposal.folder_path = Some(path);
    Ok(proposal)
}

fn named(name: Option<&str>, wanted: &str) -> bool {
    name.is_some_and(|n| n.trim().to_lowercase() == wanted.trim().to_lowercase())
}

/// The folder a path of names leads to below the root of Wallet, created where it is missing; an
/// existing folder of the same name (case ignored) is reused. An empty path is the top of Wallet
/// (`None`).
pub fn resolve_path(tx: &mut CrdtTransaction<'_>, path: &[String]) -> Result<Option<String>> {
    let mut parent = WALLET_GROUP_ID.to_owned();
    let mut any = false;
    for name in path.iter().map(|n| n.trim()).filter(|n| !n.is_empty()) {
        let existing = groups::siblings(tx, Some(&parent))?
            .into_iter()
            .find(|g| named(g.name.as_deref(), name));
        parent = match existing {
            Some(group) => group.id,
            None => groups::create_group(tx, name, None, None, None, Some(&parent))?,
        };
        any = true;
    }
    Ok(any.then_some(parent))
}
