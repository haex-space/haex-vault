//! Own-vault key transport: Tauri commands that let TypeScript hand a
//! 32-byte own-vault key to Rust and clear it on vault close.
//!
//! The key material is held in [`AppState::vault_key`] wrapped in
//! [`zeroize::Zeroizing`] so the byte buffer is scrubbed when the slot
//! is cleared or the process exits normally. Consumption of the slot by
//! [`crate::file_sync::crypto::provider::EncryptingSyncProvider`] is a
//! later PR — [`crate::file_sync::crypto::provider::FileKeySource::VaultKey`]
//! still surfaces `OwnVaultNotWired`. Landing the transport now decouples
//! that follow-up from the wire-protocol shape.
//!
//! Both commands are camelCase-renamed on the wire so the TS caller uses
//! `keyHex` (not `key_hex`).

use crate::AppState;

/// Store the caller-supplied 32-byte own-vault key. `key_hex` is a
/// lowercase-or-uppercase hex string encoding exactly 32 bytes.
///
/// Rejects malformed input (non-hex, wrong length) with a descriptive
/// error so misconfigured callers see the mistake immediately instead of
/// silently ending up with an unset slot.
#[tauri::command(rename_all = "camelCase")]
pub async fn vault_key_set(
    state: tauri::State<'_, AppState>,
    key_hex: String,
) -> Result<(), String> {
    let bytes: [u8; 32] = hex::decode(&key_hex)
        .map_err(|e| format!("vault_key_set: invalid hex: {e}"))?
        .try_into()
        .map_err(|v: Vec<u8>| format!("vault_key_set: expected 32 bytes, got {}", v.len()))?;
    let mut slot = state
        .vault_key
        .lock()
        .map_err(|e| format!("vault_key_set: lock poisoned: {e}"))?;
    *slot = Some(zeroize::Zeroizing::new(bytes));
    Ok(())
}

/// Clear the own-vault key slot. Called by TypeScript on vault-close and
/// invoked defensively by [`crate::database::create::close_database`] on
/// the Rust side too, so a hard shutdown path still scrubs the key.
#[tauri::command(rename_all = "camelCase")]
pub async fn vault_key_clear(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut slot = state
        .vault_key
        .lock()
        .map_err(|e| format!("vault_key_clear: lock poisoned: {e}"))?;
    *slot = None;
    Ok(())
}

#[cfg(test)]
mod tests;
