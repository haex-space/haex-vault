//! Tauri commands of the own-device sync (spec 024, contracts/
//! tauri-commands.md): linking a device. The main device's commands need the
//! running sync service of the open vault; the join commands run on the
//! start page, with no vault open.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use ts_rs::TS;

use crate::error::{HolziError, Result};
use crate::instances::passphrase::Passphrase;
use crate::instances::paths::get_app_local_data;
use crate::state::AppState;
use crate::sync::admission;
use crate::sync::device_list;
use crate::sync::events::{self, LINK_JOIN_STATE_CHANGED};
use crate::sync::keys;
use crate::sync::link::host_task::LinkHost;
use crate::sync::link::join_task::{JoinArgs, JoinConfig, LinkJoin};
use crate::sync::link::status::{LinkCodeInfo, LinkJoinState, LinkingStatus};
use crate::sync::link::text;
use crate::sync::registry::{SyncRegistry, SyncRuntime};
use crate::sync::servers;

/// The running service of the open vault, or `NoActiveInstance`.
fn runtime(registry: &SyncRegistry) -> Result<Arc<SyncRuntime>> {
    registry.get().ok_or(HolziError::NoActiveInstance)
}

fn host(registry: &SyncRegistry) -> Result<LinkHost> {
    Ok(runtime(registry)?.link.clone())
}

/// Shows a new link code (main device only, `NotMainDevice` otherwise).
#[tauri::command]
pub async fn link_code_create(
    state: State<'_, AppState>,
    registry: State<'_, Arc<SyncRegistry>>,
) -> Result<LinkCodeInfo> {
    let runtime = runtime(&registry)?;
    let name = state.active_name()?.ok_or(HolziError::NoActiveInstance)?;
    runtime.link.create_code(&runtime, &name).await
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct LinkTextParseArgs {
    pub text: String,
}

/// What a link text or a plain code says (spec 085): the new installation fills its form with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct LinkText {
    /// The code as shown, in groups of four.
    pub code: String,
    #[ts(optional)]
    pub vault_name: Option<String>,
    /// Absent when the text names no server: the built-in ones stay.
    #[ts(optional)]
    pub servers: Option<SyncServers>,
    /// Servers left out because they were no valid address or over a limit.
    pub dropped_servers: u32,
}

/// Reads a link text or a plain code. Works with no vault open, like `link_join_start`.
#[tauri::command]
pub fn link_text_parse(args: LinkTextParseArgs) -> Result<LinkText> {
    let parsed = text::parse(&args.text).map_err(|error| match error {
        text::LinkTextError::NotALinkCode => HolziError::NotALinkCode,
        text::LinkTextError::UnsupportedVersion => HolziError::UnsupportedVersion,
    })?;
    Ok(LinkText {
        code: parsed.code.display(),
        vault_name: parsed.vault_name,
        servers: parsed.servers.map(SyncServers::from),
        dropped_servers: parsed.dropped_servers,
    })
}

/// Withdraws the live code.
#[tauri::command]
pub async fn link_code_cancel(registry: State<'_, Arc<SyncRegistry>>) -> Result<()> {
    host(&registry)?.cancel();
    Ok(())
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct LinkConfirmArgs {
    /// Whether the new device also becomes a main device.
    pub as_main_device: bool,
}

/// The user's yes for the new device that proved the code.
#[tauri::command]
pub async fn link_confirm(
    registry: State<'_, Arc<SyncRegistry>>,
    args: LinkConfirmArgs,
) -> Result<()> {
    host(&registry)?.confirm(args.as_main_device)
}

/// The user's no.
#[tauri::command]
pub async fn link_reject(registry: State<'_, Arc<SyncRegistry>>) -> Result<()> {
    host(&registry)?.reject()
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct LinkJoinStartArgs {
    pub code: String,
    pub vault_name: String,
    pub device_name: String,
    #[ts(type = "string")]
    pub passphrase: Passphrase,
    /// The servers the vault's devices find each other through, when they are
    /// not the built-in ones; omitted `servers` means the built-in ones, while
    /// explicit `disabled` entries can switch any listed server off.
    #[serde(default)]
    #[ts(optional)]
    pub servers: Option<SyncServers>,
}

/// Starts joining a vault with a code; the vault is created here, with its
/// own passphrase, and only stays when the link finishes.
#[tauri::command]
pub async fn link_join_start(
    app: AppHandle,
    state: State<'_, AppState>,
    join: State<'_, LinkJoin>,
    args: LinkJoinStartArgs,
) -> Result<LinkJoinState> {
    state.gate().ensure_can_open()?;
    let config = match &args.servers {
        Some(servers) => {
            let config = servers::ServerConfig::from(servers.clone());
            servers::validate(&config).map_err(|reason| HolziError::InvalidInput { reason })?;
            JoinConfig::with_servers(config)
        }
        None => JoinConfig::production(),
    };
    let emit_app = app.clone();
    let emit = Arc::new(move |state: &LinkJoinState| {
        events::emit(&emit_app, LINK_JOIN_STATE_CHANGED, state.clone());
    });
    join.start(
        &app,
        emit,
        JoinArgs {
            code: args.code,
            vault_name: args.vault_name,
            device_name: args.device_name,
            passphrase: args.passphrase,
        },
        config,
    )
    .await
}

/// Where the join stands.
#[tauri::command]
pub async fn link_join_status(join: State<'_, LinkJoin>) -> Result<LinkJoinState> {
    join.status().ok_or_else(|| HolziError::InvalidInput {
        reason: "no link was started".into(),
    })
}

/// Cancels the join; its vault is removed.
#[tauri::command]
pub async fn link_join_cancel(join: State<'_, LinkJoin>) -> Result<()> {
    join.cancel().await;
    Ok(())
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct DeviceRemoveArgs {
    /// The device key as hex.
    pub device_pubkey: String,
}

/// Reads a device key written as 64 hex characters.
fn parse_device(text: &str) -> Result<[u8; 32]> {
    crate::sync::presence::decode_hex(text)
        .ok()
        .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
        .ok_or_else(|| HolziError::InvalidInput {
            reason: "a device key has 64 hex characters".into(),
        })
}

/// Removes a device from the vault (spec 024, FR-026 to FR-028). Only a main
/// device may, and never for itself; the backend checks, not only the view.
#[tauri::command]
pub async fn device_remove(
    registry: State<'_, Arc<SyncRegistry>>,
    args: DeviceRemoveArgs,
) -> Result<()> {
    let target = parse_device(&args.device_pubkey)?;
    remove_device_now(&runtime(&registry)?, target).await
}

/// What `device_remove` does once the running service is at hand: publishes
/// the removal, ends the device's session and forgets its address, and
/// tells the others and the view.
pub async fn remove_device_now(runtime: &Arc<SyncRuntime>, target: [u8; 32]) -> Result<()> {
    let (replica, own) = (Arc::clone(&runtime.replica), runtime.keys.clone());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let removal = tokio::task::spawn_blocking(move || {
        crate::sync::removal::remove_device(&replica, &own, &target, now)
    })
    .await
    .map_err(crate::sync::removal::RemovalError::from)??;
    runtime.node.forget(target, removal.endpoint_id);
    // The list changed behind `VaultDb`: wake the session and presence as a
    // commit would, so the others hear of it at once, and tell the view.
    (runtime.wake)();
    runtime.node.announce_devices_changed();
    Ok(())
}

/// What this device is in the vault (spec 024, FR-034).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "snake_case")]
pub enum ThisDevice {
    Main,
    Linked,
    /// Not on the device list yet: a copy of the vault waits for a main
    /// device to admit it (FR-044).
    AwaitingAdmission,
    /// The list says it was removed and it no longer syncs (FR-027).
    Removed,
}

/// One open request to join the vault (FR-045).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct OpenAdmission {
    pub device_pubkey: String,
    pub name: String,
    #[ts(type = "number")]
    pub requested_at: u64,
}

/// What the device list view needs besides the devices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub this_device: ThisDevice,
    pub open_admissions: Vec<OpenAdmission>,
    pub linking: Option<LinkingStatus>,
    /// This vault file is the copy of a main device and this device enrolled
    /// itself as a main device; the user has not read that yet (FR-044).
    pub copy_enrolled_as_main: bool,
}

/// The public key of the vault identity (FR-046).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct VaultPublicIdentity {
    /// Nostr `npub` form, naming the vault in member lists and rights; not an invitation address.
    pub npub: String,
    pub hex: String,
}

/// The servers devices find each other through (FR-008).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct SyncServers {
    /// The servers added besides the built-in ones.
    pub nostr_relays: Vec<String>,
    pub iroh_relays: Vec<String>,
    /// The servers, built-in or added, of either kind, that are switched off.
    pub disabled: Vec<String>,
}

impl From<SyncServers> for servers::ServerConfig {
    fn from(args: SyncServers) -> Self {
        Self {
            nostr_relays: args.nostr_relays,
            iroh_relays: args.iroh_relays,
            disabled: args.disabled,
        }
    }
}

impl From<servers::ServerConfig> for SyncServers {
    fn from(config: servers::ServerConfig) -> Self {
        Self {
            nostr_relays: config.nostr_relays,
            iroh_relays: config.iroh_relays,
            disabled: config.disabled,
        }
    }
}

/// Reads this device's place in the vault from the lists and the vault
/// secret.
pub fn this_device(
    q: &mut impl crate::storage::query::Query,
    own: &[u8; 32],
) -> haex_crdt::Result<ThisDevice> {
    let vault = keys::vault_pubkey(q)?.unwrap_or([0; 32]);
    let valid = device_list::valid_lists(&device_list::load_all(q)?, &vault);
    let Some(effective) = device_list::effective(&valid) else {
        return Ok(ThisDevice::AwaitingAdmission);
    };
    if effective.list.removes(own) {
        return Ok(ThisDevice::Removed);
    }
    Ok(match effective.list.device(own) {
        Some(device) if device.role == device_list::Role::Main => ThisDevice::Main,
        Some(_) => ThisDevice::Linked,
        None => ThisDevice::AwaitingAdmission,
    })
}

/// Reads the vault's public identity as `npub` and hex.
pub fn public_identity(
    q: &mut impl crate::storage::query::Query,
) -> haex_crdt::Result<Option<VaultPublicIdentity>> {
    use nostr::nips::nip19::ToBech32;
    let Some(pubkey) = keys::vault_pubkey(q)? else {
        return Ok(None);
    };
    let key = nostr::key::PublicKey::from_slice(&pubkey)
        .map_err(|e| haex_crdt::Error::consumer(e.to_string()))?;
    // Encoding a public key cannot fail: `ToBech32::Err` is `Infallible`, so
    // the `Ok` pattern is irrefutable. Mapping the error would call a method
    // on an uninhabited value, which newer rustc flags as unreachable code.
    let Ok(npub) = key.to_bech32();
    Ok(Some(VaultPublicIdentity {
        npub,
        hex: keys::hex(&pubkey),
    }))
}

/// Who is this device, what waits for an answer, and is a link going on.
#[tauri::command]
pub async fn sync_status(
    app: AppHandle,
    state: State<'_, AppState>,
    registry: State<'_, Arc<SyncRegistry>>,
) -> Result<SyncStatus> {
    let installation_id_file = crate::identity::installation_id_path(&get_app_local_data(&app)?);
    let installation = crate::identity::read_or_mint_installation_uuid(&installation_id_file)
        .map_err(HolziError::from)?;
    let db = crate::state_utils::active_database(&state)?;
    let device = db.device_id();
    let (this, open_admissions, copy_enrolled_as_main) = db
        .read(move |r| {
            let this = match keys::load_device_keys(r, installation)? {
                Some(own) => this_device(r, &own.device_pubkey)?,
                None => ThisDevice::AwaitingAdmission,
            };
            Ok((this, open_admissions(r)?, copy_notice_pending(r, device)?))
        })
        .await?;
    Ok(SyncStatus {
        this_device: this,
        open_admissions,
        linking: registry.get().and_then(|runtime| runtime.link.status()),
        copy_enrolled_as_main,
    })
}

/// The requests of copies to join the vault that wait for a decision.
fn open_admissions(
    q: &mut impl crate::storage::query::Query,
) -> haex_crdt::Result<Vec<OpenAdmission>> {
    let Some(vault) = keys::vault_pubkey(q)? else {
        return Ok(Vec::new());
    };
    let valid = device_list::valid_lists(&device_list::load_all(q)?, &vault);
    let Some(effective) = device_list::effective(&valid) else {
        return Ok(Vec::new());
    };
    Ok(admission::load_open(q, &admission::settled(effective))?
        .into_iter()
        .map(|open| OpenAdmission {
            device_pubkey: keys::hex(&open.device),
            name: open.name,
            requested_at: open.requested_at,
        })
        .collect())
}

/// Whether this device's notice that it enrolled as a main device is still
/// unread.
fn copy_notice_pending(
    q: &mut impl crate::storage::query::Query,
    device: uuid::Uuid,
) -> haex_crdt::Result<bool> {
    Ok(crate::storage::preferences::get(
        q,
        crate::storage::preferences::PrefScope::Device(device),
        admission::PREF_ENROLLED_AS_MAIN,
    )?
    .is_some())
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct AdmissionDecideArgs {
    /// The key of the copy as hex.
    pub device_pubkey: String,
    /// `true` admits the copy, `false` keeps it out.
    pub admit: bool,
}

/// A main device's answer to the request of a copy of the vault file
/// (spec 024, FR-045). Nothing admits a copy without it; the backend checks
/// that this is a main device, not only the view.
#[tauri::command]
pub async fn admission_decide(
    registry: State<'_, Arc<SyncRegistry>>,
    args: AdmissionDecideArgs,
) -> Result<()> {
    let target = parse_device(&args.device_pubkey)?;
    decide_admission_now(&runtime(&registry)?, target, args.admit).await
}

/// What `admission_decide` does once the running service is at hand: stores
/// the decision, and for an admission wakes the session and presence, so the
/// copy hears of it at once, and tells the view.
pub async fn decide_admission_now(
    runtime: &Arc<SyncRuntime>,
    target: [u8; 32],
    admit: bool,
) -> Result<()> {
    let (replica, own) = (Arc::clone(&runtime.replica), runtime.keys.clone());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    tokio::task::spawn_blocking(move || admission::decide(&replica, &own, &target, admit, now))
        .await
        .map_err(admission::AdmissionError::from)??;
    (runtime.wake)();
    runtime.node.announce_devices_changed();
    Ok(())
}

/// The user read that this copy enrolled itself as a main device.
#[tauri::command]
pub async fn sync_copy_notice_dismiss(state: State<'_, AppState>) -> Result<()> {
    let db = crate::state_utils::active_database(&state)?;
    let device = db.device_id();
    db.write(move |tx| {
        crate::storage::preferences::delete(
            tx,
            crate::storage::preferences::PrefScope::Device(device),
            admission::PREF_ENROLLED_AS_MAIN,
        )?;
        Ok(())
    })
    .await?;
    Ok(())
}

/// The public key of the vault identity; `NoActiveInstance` without an open
/// vault, an error when the vault has none yet.
#[tauri::command]
pub async fn vault_public_identity(state: State<'_, AppState>) -> Result<VaultPublicIdentity> {
    let db = crate::state_utils::active_database(&state)?;
    db.read(|r| public_identity(r))
        .await?
        .ok_or_else(|| HolziError::InvalidInput {
            reason: "the vault has no identity yet".into(),
        })
}

/// The built-in servers, as `nostrRelays` and `irohRelays` (nothing is
/// switched off). Needs no vault, so the landing page's link form can show
/// them too.
#[tauri::command]
pub fn sync_servers_defaults() -> SyncServers {
    SyncServers {
        nostr_relays: servers::default_nostr_relays(),
        iroh_relays: servers::default_iroh_relays(),
        disabled: Vec::new(),
    }
}

/// The servers as stored: the ones added and the ones switched off. The
/// built-in servers are listed by `sync_servers_defaults`.
#[tauri::command]
pub async fn sync_servers_get(state: State<'_, AppState>) -> Result<SyncServers> {
    let db = crate::state_utils::active_database(&state)?;
    let config = db.read(|r| servers::read(r)).await?;
    Ok(config.into())
}

/// Stores the servers. The iroh relays apply at once; the Nostr relays when
/// the vault is opened next.
#[tauri::command]
pub async fn sync_servers_set(
    state: State<'_, AppState>,
    registry: State<'_, Arc<SyncRegistry>>,
    args: SyncServers,
) -> Result<()> {
    let config = servers::ServerConfig::from(args);
    servers::validate(&config).map_err(|reason| HolziError::InvalidInput { reason })?;
    let _servers_guard = state.lock_sync_servers().await;
    let db = crate::state_utils::active_database(&state)?;
    let stored = config.clone();
    db.write(move |tx| servers::write(tx, &stored)).await?;
    if let Some(runtime) = registry.get() {
        runtime.node.apply_relays(&config.relay_mode()).await;
    }
    Ok(())
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
