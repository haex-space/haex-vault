//! The marketplaces of a vault in `haex_marketplaces` (spec 069, data-model.md, research R10,
//! R14). Every row is equal and can be changed and switched off, the preset included; every row
//! but the preset can be deleted, so the public marketplace is always there.
//! Rows are written the way the other tables are: check, then insert or update, never an upsert
//! (an upsert fires two CRDT triggers), and without a UNIQUE constraint.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use url::Url;
use uuid::Uuid;

use super::kinds;
use super::model::{MarketplaceAuthType, MarketplaceInput};
use crate::error::{HolziError, Result};
use crate::storage::query::Query;

/// The id of the preset row: the public marketplace of the project. A fixed id lets two devices
/// that both add it end up with one row (research R10).
pub const PRESET_ID: &str = "preset-public";
/// The name of the preset row.
pub const PRESET_NAME: &str = "Haex Marketplace";
/// The address of the public marketplace. **The only place of this address**: the rest of the
/// host knows no marketplace but the rows of the table (FR-034, checked by a test).
pub const PRESET_URL: &str = "https://marketplace.haex.space";

/// The environment variable that points the preset at a local test marketplace.
const E2E_MARKETPLACE_URL: &str = "HOLZI_E2E_MARKETPLACE_URL";

/// The address of the preset: `PRESET_URL`, or in a debug build (the E2E suite builds one) the
/// value of `HOLZI_E2E_MARKETPLACE_URL` when it is set. A release build ignores the variable
/// (research R15).
pub fn preset_url() -> String {
    preset_url_for(
        cfg!(debug_assertions),
        std::env::var(E2E_MARKETPLACE_URL).ok(),
    )
}

/// [`preset_url`] with the build kind and the variable given, so a test needs no rebuild.
pub fn preset_url_for(debug_build: bool, e2e_override: Option<String>) -> String {
    match e2e_override {
        Some(url) if debug_build && !url.trim().is_empty() => url.trim().to_owned(),
        _ => PRESET_URL.to_owned(),
    }
}

/// One row of `haex_marketplaces`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketplaceRow {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub enabled: bool,
    pub sort_order: i64,
    pub auth_type: MarketplaceAuthType,
    pub credentials_item_id: Option<String>,
}

const COLUMNS: &str = "id, name, base_url, enabled, sort_order, auth_type, credentials_item_id";

fn from_row(row: &haex_crdt::rusqlite::Row<'_>) -> haex_crdt::rusqlite::Result<MarketplaceRow> {
    Ok(MarketplaceRow {
        id: row.get(0)?,
        name: row.get(1)?,
        base_url: row.get(2)?,
        enabled: row.get::<_, i64>(3)? != 0,
        sort_order: row.get(4)?,
        auth_type: MarketplaceAuthType::from_stored(&row.get::<_, String>(5)?),
        credentials_item_id: row.get(6)?,
    })
}

fn refused(kind: &str) -> HolziError {
    HolziError::Marketplace {
        reason: kind.to_owned(),
    }
}

/// The address a marketplace is kept under: `http` or `https`, a host, a path prefix if there is
/// one, no trailing `/`, no query, no fragment and no login in the address (FR-024).
pub fn normalize_base_url(raw: &str) -> Result<String> {
    let mut url = Url::parse(raw.trim()).map_err(|_| refused(kinds::INVALID_URL))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(refused(kinds::INVALID_URL));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(refused(kinds::INVALID_URL));
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

/// Whether a login may be sent to `base_url` (FR-027): only over `https`. A debug build, which the
/// end-to-end suite uses, also takes `http` to a loopback address, because its test marketplace has
/// no certificate (research R15); a release build never does.
pub fn login_transport_ok(base_url: &str) -> bool {
    login_transport_ok_for(cfg!(debug_assertions), base_url)
}

/// [`login_transport_ok`] with the build kind given, so a test covers the release rule without a
/// release build.
pub fn login_transport_ok_for(debug_build: bool, base_url: &str) -> bool {
    let Ok(url) = Url::parse(base_url) else {
        return false;
    };
    match url.scheme() {
        "https" => true,
        "http" => {
            debug_build
                && match url.host() {
                    Some(url::Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
                    Some(url::Host::Ipv4(address)) => address.is_loopback(),
                    Some(url::Host::Ipv6(address)) => address.is_loopback(),
                    None => false,
                }
        }
        _ => false,
    }
}

/// A marketplace that passed the rules of the data model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    pub name: String,
    pub base_url: String,
    pub auth_type: MarketplaceAuthType,
    pub credentials_item_id: Option<String>,
}

/// Checks the form's input (data-model.md "Rules"): a name, a valid address, and for a login an
/// entry of the password manager and an `https` address (FR-027).
pub fn check(input: &MarketplaceInput) -> Result<Checked> {
    check_with(input, input.new_credentials.is_some())
}

/// [`check`], where `login_follows` says an entry for the login is created from the form's typed
/// values once the rules hold (so the row needs no id yet).
pub fn check_with(input: &MarketplaceInput, login_follows: bool) -> Result<Checked> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(HolziError::InvalidInput {
            reason: "marketplace name".to_owned(),
        });
    }
    let base_url = normalize_base_url(&input.base_url)?;
    let credentials_item_id = input
        .credentials_item_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned);
    let credentials_item_id = if input.auth_type.needs_credentials() {
        if credentials_item_id.is_none() && !login_follows {
            return Err(refused(kinds::CREDENTIALS_REQUIRED));
        }
        if !login_transport_ok(&base_url) {
            return Err(refused(kinds::INSECURE_CREDENTIALS));
        }
        credentials_item_id
    } else {
        None
    };
    Ok(Checked {
        name: name.to_owned(),
        base_url,
        auth_type: input.auth_type,
        credentials_item_id,
    })
}

/// Every marketplace in the order the user chose: `(sort_order, name, id)`. A row whose address
/// does not hold up (written by a newer version) is left out and logged, not an error.
pub fn list(q: &mut impl Query) -> Result<Vec<MarketplaceRow>> {
    let sql = format!("SELECT {COLUMNS} FROM haex_marketplaces ORDER BY sort_order, name, id");
    let rows = q.query_map(&sql, &[], from_row)?;
    Ok(rows
        .into_iter()
        .filter(|row| {
            let usable = normalize_base_url(&row.base_url).is_ok();
            if !usable {
                log::warn!(
                    "marketplace {}: the address does not hold up, the row is left out",
                    row.id
                );
            }
            usable
        })
        .collect())
}

/// Whether a marketplace points at the password entry `item_id`.
pub fn credentials_in_use(q: &mut impl Query, item_id: &str) -> Result<bool> {
    Ok(q.query_row(
        "SELECT COUNT(*) FROM haex_marketplaces WHERE credentials_item_id = ?1",
        params![item_id],
        |r| r.get::<_, i64>(0),
    )?
    .unwrap_or(0)
        > 0)
}

/// One marketplace.
pub fn get(q: &mut impl Query, id: &str) -> Result<Option<MarketplaceRow>> {
    let sql = format!("SELECT {COLUMNS} FROM haex_marketplaces WHERE id = ?1");
    Ok(q.query_row(&sql, params![id], from_row)?)
}

fn next_sort_order(tx: &mut CrdtTransaction<'_>) -> Result<i64> {
    let highest = tx
        .query_row("SELECT MAX(sort_order) FROM haex_marketplaces", &[], |r| {
            r.get::<_, Option<i64>>(0)
        })?
        .flatten();
    Ok(highest.map_or(1, |value| value + 1))
}

/// Creates a marketplace (no `id`) at the end of the list, or changes one (`id` given). The new
/// id is a random UUID; only the preset has a fixed one. Returns the id.
pub fn save(tx: &mut CrdtTransaction<'_>, input: &MarketplaceInput, now: i64) -> Result<String> {
    let checked = check(input)?;
    match input.id.as_deref() {
        Some(id) => {
            if get(tx, id)?.is_none() {
                return Err(refused(kinds::NOT_FOUND));
            }
            tx.execute(
                "UPDATE haex_marketplaces SET name = ?2, base_url = ?3, auth_type = ?4, \
                 credentials_item_id = ?5, updated_at = ?6 WHERE id = ?1",
                params![
                    id,
                    checked.name,
                    checked.base_url,
                    checked.auth_type.as_str(),
                    checked.credentials_item_id,
                    now,
                ],
            )?;
            Ok(id.to_owned())
        }
        None => {
            let id = Uuid::new_v4().to_string();
            let sort_order = next_sort_order(tx)?;
            insert(tx, &id, &checked, sort_order, now)?;
            Ok(id)
        }
    }
}

fn insert(
    tx: &mut CrdtTransaction<'_>,
    id: &str,
    checked: &Checked,
    sort_order: i64,
    now: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO haex_marketplaces (id, name, base_url, enabled, sort_order, auth_type, \
         credentials_item_id, created_at, updated_at) VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6, ?7, ?7)",
        params![
            id,
            checked.name,
            checked.base_url,
            sort_order,
            checked.auth_type.as_str(),
            checked.credentials_item_id,
            now,
        ],
    )?;
    Ok(())
}

/// Switches a marketplace on or off (applies at once, FR-020).
pub fn set_enabled(tx: &mut CrdtTransaction<'_>, id: &str, enabled: bool, now: i64) -> Result<()> {
    if get(tx, id)?.is_none() {
        return Err(refused(kinds::NOT_FOUND));
    }
    tx.execute(
        "UPDATE haex_marketplaces SET enabled = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, i64::from(enabled), now],
    )?;
    Ok(())
}

/// Writes the whole order: the marketplace at position `n` of `ids` gets `sort_order = n + 1`.
/// A row that is not in `ids` (added on another device meanwhile) keeps its place behind them.
pub fn reorder(tx: &mut CrdtTransaction<'_>, ids: &[String], now: i64) -> Result<()> {
    for (index, id) in ids.iter().enumerate() {
        let position = i64::try_from(index).unwrap_or(i64::MAX - 1) + 1;
        tx.execute(
            "UPDATE haex_marketplaces SET sort_order = ?2, updated_at = ?3 \
             WHERE id = ?1 AND sort_order <> ?2",
            params![id, position, now],
        )?;
    }
    Ok(())
}

/// Whether a marketplace can be deleted: every one but the preset, which is only switched off
/// (FR-023).
pub fn deletable(id: &str) -> bool {
    id != PRESET_ID
}

/// Deletes a marketplace (FR-023); the preset is refused. Installed extensions stay.
pub fn delete(tx: &mut CrdtTransaction<'_>, id: &str) -> Result<()> {
    if !deletable(id) {
        return Err(refused(kinds::PRESET_NOT_DELETABLE));
    }
    if get(tx, id)?.is_none() {
        return Err(refused(kinds::NOT_FOUND));
    }
    tx.execute("DELETE FROM haex_marketplaces WHERE id = ?1", params![id])?;
    Ok(())
}

/// Adds the preset row if no row of that id exists; otherwise changes nothing, not even a switch
/// the user turned off (FR-022). Called by [`super::seed::ensure_preset`]. Returns the row.
pub fn add_preset(tx: &mut CrdtTransaction<'_>, now: i64) -> Result<MarketplaceRow> {
    if let Some(existing) = get(tx, PRESET_ID)? {
        return Ok(existing);
    }
    let checked = Checked {
        name: PRESET_NAME.to_owned(),
        base_url: normalize_base_url(&preset_url())?,
        auth_type: MarketplaceAuthType::None,
        credentials_item_id: None,
    };
    let sort_order = next_sort_order(tx)?;
    insert(tx, PRESET_ID, &checked, sort_order, now)?;
    get(tx, PRESET_ID)?.ok_or_else(|| refused(kinds::NOT_FOUND))
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
