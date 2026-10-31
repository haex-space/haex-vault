//! The invoke-handler wrapper: the one place every request passes (spec 013, research R3).
//!
//! Once a close has started, every command except a short app-scoped allow-list is rejected at
//! once with `VaultClosed` and its body never runs. The rule is default-deny: a command added
//! later is gated without anyone remembering to do so.

use tauri::ipc::Invoke;
use tauri::Runtime;

use super::VaultGate;
use crate::error::HolziError;

/// Commands that never touch the vault and stay callable after a close starts. Confirmed one by
/// one in research R3; `close_instance` is here because a close must always be accepted (FR-002).
pub const APP_SCOPED_COMMANDS: &[&str] = &[
    "active_instance_name",
    "close_instance",
    "list_instances",
    "get_hardware_info",
    "list_catalog",
    "catalog_recommend_tiers",
    "list_stt_catalog",
    "stt_recommend_tiers",
    // Spec 066: a fixed table like the STT catalog.
    "list_tts_catalog",
    // Spec 043: a fixed table of the platform; the vault picker needs it too.
    "platform_capabilities",
];

impl VaultGate {
    /// Wraps the generated invoke handler so a closing gate rejects every command that is not on
    /// the allow-list.
    pub fn wrap<R: Runtime>(
        &self,
        handler: impl Fn(Invoke<R>) -> bool + Send + Sync + 'static,
    ) -> impl Fn(Invoke<R>) -> bool + Send + Sync + 'static {
        let gate = self.clone();
        move |invoke: Invoke<R>| {
            if gate.is_closing() && !APP_SCOPED_COMMANDS.contains(&invoke.message.command()) {
                invoke.resolver.reject(HolziError::VaultClosed);
                return true;
            }
            handler(invoke)
        }
    }
}
