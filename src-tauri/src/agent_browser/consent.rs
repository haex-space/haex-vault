//! Declining the optional cookies of consent banners (spec 065 FR-065, FR-066, research R23): the
//! script `consent.js` is added to every document of an agent's tab, frames included, and finds
//! and presses the button that rejects. It never accepts. This module installs it and lets the
//! reading of a page wait the moment it needs, so the banner is gone from what the agent reads.
//! What happened comes to the agent as `consent: "declined"`, `"open"` or `"wall"` (the page takes
//! no choice without a consent that is not optional) on its next result.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;

use super::cdp::{page, Connection};
use super::error::Result;
use super::read;
use super::tab::Tab;

const SCRIPT: &str = include_str!("consent.js");

fn yes() -> bool {
    true
}

/// What the user chose for cookie banners (FR-066): declining the optional cookies is on until it
/// is turned off. Kept in the vault ([`super::choices`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ConsentSettings {
    #[serde(default = "yes")]
    pub reject_optional: bool,
}

impl Default for ConsentSettings {
    fn default() -> Self {
        Self {
            reject_optional: true,
        }
    }
}

/// How long the reading of a page waits for the search of the script to end at most.
const MAX_WAIT: Duration = Duration::from_millis(2000);
const STEP: Duration = Duration::from_millis(100);
/// How long a banner gets to leave after it was declined.
const LEAVES: Duration = Duration::from_millis(350);

/// Adds the script to every new document of the tab in `session`, the current one included.
pub async fn install(connection: &Connection, session: &str) -> Result<()> {
    page::add_script(connection, session, SCRIPT).await
}

/// What the script found in the main document of `tab`: `searching`, `none`, `declined`, `open`,
/// `wall`, or `off` while it is not installed.
async fn state(connection: &Connection, tab: &Tab) -> String {
    read::script(connection, &tab.session, &json!({ "consentState": true }))
        .await
        .ok()
        .and_then(|reply| reply["state"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "off".to_owned())
}

/// Waits until the script has found its banner or has seen that there is none (at most
/// [`MAX_WAIT`]), and a moment longer when it declined one, so the page is read without it.
pub(super) async fn settle(connection: &Connection, tab: &Tab) {
    let until = Instant::now() + MAX_WAIT;
    loop {
        match state(connection, tab).await.as_str() {
            "searching" if Instant::now() < until => tokio::time::sleep(STEP).await,
            "declined" => {
                tokio::time::sleep(LEAVES).await;
                return;
            }
            _ => return,
        }
    }
}
