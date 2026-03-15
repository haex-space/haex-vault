//! Tauri commands for extension shared space management.
//!
//! These commands allow extensions to:
//! - Assign/unassign their table rows to shared spaces for selective sync
//! - List available sync backends
//! - List shared spaces the user is a member of
//! - Create new shared spaces
//!
//! Auth: Space management commands use a JWT stored in AppState.auth_token,
//! synced from the frontend Supabase session via set_auth_token.

use crate::database::core::{self, with_connection};
use crate::database::error::DatabaseError;
use crate::database::row::{get_bool, get_string};
use crate::database::DbConnection;
use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::SpaceAction;
use crate::extension::utils::{
    emit_permission_prompt_if_needed, get_extension_table_prefix, resolve_extension_id,
};
use crate::table_names::TABLE_SHARED_SPACE_SYNC;
use crate::AppState;

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use p256::pkcs8::{DecodePublicKey, EncodePublicKey};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, State, WebviewWindow};

/// Extract an i64 from a JSON row value.
fn get_i64(row: &[serde_json::Value], idx: usize) -> i64 {
    row.get(idx).and_then(|v| v.as_i64()).unwrap_or(0)
}

/// A single row assignment to a shared space.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceAssignment {
    pub table_name: String,
    pub row_pks: String,
    pub space_id: String,
}

/// Result of a space assignment query.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceAssignmentRow {
    pub table_name: String,
    pub row_pks: String,
    pub space_id: String,
}

/// Validates that all table names in the assignments start with the extension's prefix.
fn validate_table_prefixes(
    assignments: &[SpaceAssignment],
    prefix: &str,
) -> Result<(), ExtensionError> {
    for assignment in assignments {
        if !assignment.table_name.starts_with(prefix) {
            return Err(ExtensionError::SecurityViolation {
                reason: format!(
                    "Table '{}' does not belong to this extension (expected prefix '{}')",
                    assignment.table_name, prefix
                ),
            });
        }
    }
    Ok(())
}

/// Validates that a single table name starts with the extension's prefix.
fn validate_single_table_prefix(
    table_name: &str,
    prefix: &str,
) -> Result<(), ExtensionError> {
    if !table_name.starts_with(prefix) {
        return Err(ExtensionError::SecurityViolation {
            reason: format!(
                "Table '{}' does not belong to this extension (expected prefix '{}')",
                table_name, prefix
            ),
        });
    }
    Ok(())
}

/// Bulk assign rows to shared spaces (INSERT OR IGNORE).
///
/// Extensions can only assign rows from their own tables (validated via prefix).
#[tauri::command]
pub async fn extension_space_assign(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    assignments: Vec<SpaceAssignment>,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<u64, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let perm_result =
        PermissionManager::check_spaces_permission(&state, &extension_id, SpaceAction::ReadWrite)
            .await;
    if let Err(ref e) = perm_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    perm_result?;

    let extension = state
        .extension_manager
        .get_extension(&extension_id)
        .ok_or_else(|| ExtensionError::ValidationError {
            reason: format!("Extension with ID {} not found", extension_id),
        })?;

    let prefix = get_extension_table_prefix(
        &extension.manifest.public_key,
        &extension.manifest.name,
    );

    validate_table_prefixes(&assignments, &prefix)?;

    if assignments.is_empty() {
        return Ok(0);
    }

    let total_inserted = with_connection(&state.db, |conn| {
        let mut inserted: u64 = 0;
        for assignment in &assignments {
            let sql = format!(
                "INSERT OR IGNORE INTO {} (table_name, row_pks, space_id) VALUES (?1, ?2, ?3)",
                TABLE_SHARED_SPACE_SYNC
            );
            let rows = conn
                .execute(
                    &sql,
                    rusqlite::params![
                        assignment.table_name,
                        assignment.row_pks,
                        assignment.space_id,
                    ],
                )
                .map_err(DatabaseError::from)?;
            inserted += rows as u64;
        }
        Ok(inserted)
    })?;

    Ok(total_inserted)
}

/// Bulk unassign rows from shared spaces (DELETE).
///
/// Extensions can only unassign rows from their own tables (validated via prefix).
#[tauri::command]
pub async fn extension_space_unassign(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    assignments: Vec<SpaceAssignment>,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<u64, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let perm_result =
        PermissionManager::check_spaces_permission(&state, &extension_id, SpaceAction::ReadWrite)
            .await;
    if let Err(ref e) = perm_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    perm_result?;

    let extension = state
        .extension_manager
        .get_extension(&extension_id)
        .ok_or_else(|| ExtensionError::ValidationError {
            reason: format!("Extension with ID {} not found", extension_id),
        })?;

    let prefix = get_extension_table_prefix(
        &extension.manifest.public_key,
        &extension.manifest.name,
    );

    validate_table_prefixes(&assignments, &prefix)?;

    if assignments.is_empty() {
        return Ok(0);
    }

    let total_deleted = with_connection(&state.db, |conn| {
        let mut deleted: u64 = 0;
        for assignment in &assignments {
            let sql = format!(
                "DELETE FROM {} WHERE table_name = ?1 AND row_pks = ?2 AND space_id = ?3",
                TABLE_SHARED_SPACE_SYNC
            );
            let rows = conn
                .execute(
                    &sql,
                    rusqlite::params![
                        assignment.table_name,
                        assignment.row_pks,
                        assignment.space_id,
                    ],
                )
                .map_err(DatabaseError::from)?;
            deleted += rows as u64;
        }
        Ok(deleted)
    })?;

    Ok(total_deleted)
}

/// Get space assignments for an extension's table, optionally filtered by row PKs.
///
/// Extensions can only query assignments for their own tables (validated via prefix).
#[tauri::command]
pub async fn extension_space_get_assignments(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    table_name: String,
    row_pks: Option<Vec<String>>,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<Vec<SpaceAssignmentRow>, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let perm_result =
        PermissionManager::check_spaces_permission(&state, &extension_id, SpaceAction::Read)
            .await;
    if let Err(ref e) = perm_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    perm_result?;

    let extension = state
        .extension_manager
        .get_extension(&extension_id)
        .ok_or_else(|| ExtensionError::ValidationError {
            reason: format!("Extension with ID {} not found", extension_id),
        })?;

    let prefix = get_extension_table_prefix(
        &extension.manifest.public_key,
        &extension.manifest.name,
    );

    validate_single_table_prefix(&table_name, &prefix)?;

    let rows = with_connection(&state.db, |conn| {
        match &row_pks {
            Some(pks) if !pks.is_empty() => {
                let placeholders: Vec<String> =
                    (2..=pks.len() + 1).map(|i| format!("?{}", i)).collect();
                let sql = format!(
                    "SELECT table_name, row_pks, space_id FROM {} WHERE table_name = ?1 AND row_pks IN ({})",
                    TABLE_SHARED_SPACE_SYNC,
                    placeholders.join(", ")
                );

                let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
                params.push(Box::new(table_name.clone()));
                for pk in pks {
                    params.push(Box::new(pk.clone()));
                }

                let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                    params.iter().map(|p| p.as_ref()).collect();

                let mut stmt = conn.prepare(&sql).map_err(DatabaseError::from)?;
                let result = stmt
                    .query_map(param_refs.as_slice(), |row| {
                        Ok(SpaceAssignmentRow {
                            table_name: row.get(0)?,
                            row_pks: row.get(1)?,
                            space_id: row.get(2)?,
                        })
                    })
                    .map_err(DatabaseError::from)?;

                result
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(DatabaseError::from)
            }
            _ => {
                let sql = format!(
                    "SELECT table_name, row_pks, space_id FROM {} WHERE table_name = ?1",
                    TABLE_SHARED_SPACE_SYNC
                );

                let mut stmt = conn.prepare(&sql).map_err(DatabaseError::from)?;
                let result = stmt
                    .query_map([&table_name], |row| {
                        Ok(SpaceAssignmentRow {
                            table_name: row.get(0)?,
                            row_pks: row.get(1)?,
                            space_id: row.get(2)?,
                        })
                    })
                    .map_err(DatabaseError::from)?;

                result
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(DatabaseError::from)
            }
        }
    })?;

    Ok(rows)
}

// ============================================================================
// Auth Token Management
// ============================================================================

/// Store the frontend Supabase JWT in AppState for use by space commands.
#[tauri::command]
pub async fn set_auth_token(
    state: State<'_, AppState>,
    token: Option<String>,
) -> Result<(), String> {
    *state
        .auth_token
        .lock()
        .map_err(|e| format!("Failed to lock auth_token: {}", e))? = token;
    Ok(())
}

/// Read the stored auth token from AppState.
fn get_auth_token(state: &AppState) -> Result<String, ExtensionError> {
    state
        .auth_token
        .lock()
        .map_err(|e| ExtensionError::MutexPoisoned {
            reason: e.to_string(),
        })?
        .clone()
        .ok_or_else(|| ExtensionError::ValidationError {
            reason: "Not authenticated — no auth token available".to_string(),
        })
}

// ============================================================================
// Space Management Commands
// ============================================================================

/// Minimal sync backend info exposed to extensions.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncBackendInfo {
    pub id: String,
    pub name: String,
    pub server_url: String,
    pub is_default: bool,
}

/// A shared space with its decrypted name.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecryptedSpace {
    pub id: String,
    pub name: String,
    pub role: String,
    pub server_url: String,
    pub created_at: String,
}

/// Raw space data as returned by GET /spaces.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServerSpace {
    id: String,
    encrypted_name: String,
    name_nonce: String,
    current_key_generation: i64,
    role: String,
    created_at: String,
}

/// List available sync backends that can host shared spaces.
#[tauri::command]
pub async fn extension_space_list_backends(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<Vec<SyncBackendInfo>, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let perm_result =
        PermissionManager::check_spaces_permission(&state, &extension_id, SpaceAction::Read)
            .await;
    if let Err(ref e) = perm_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    perm_result?;

    let rows = core::select_with_crdt(
        "SELECT id, name, server_url, enabled, priority FROM haex_sync_backends".to_string(),
        vec![],
        &state.db,
    )
    .map_err(|e| ExtensionError::Database {
        source: DatabaseError::DatabaseError {
            reason: e.to_string(),
        },
    })?;

    let mut backends: Vec<(SyncBackendInfo, i64)> = rows
        .iter()
        .filter(|row| {
            let enabled = get_bool(row, 3);
            let url = get_string(row, 2);
            enabled && !url.is_empty()
        })
        .map(|row| {
            let priority = get_i64(row, 4);
            (
                SyncBackendInfo {
                    id: get_string(row, 0),
                    name: get_string(row, 1),
                    server_url: get_string(row, 2),
                    is_default: false,
                },
                priority,
            )
        })
        .collect();

    backends.sort_by(|a, b| b.1.cmp(&a.1));

    let default_id = backends.first().map(|(b, _)| b.id.clone());
    let result: Vec<SyncBackendInfo> = backends
        .into_iter()
        .map(|(mut b, _)| {
            b.is_default = Some(&b.id) == default_id.as_ref();
            b
        })
        .collect();

    Ok(result)
}

/// Fetch spaces from a sync server using a bearer token.
async fn fetch_spaces(
    client: &reqwest::Client,
    server_url: &str,
    token: &str,
) -> Result<Vec<ServerSpace>, ExtensionError> {
    let url = format!("{}/spaces", server_url.trim_end_matches('/'));
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| ExtensionError::ValidationError {
            reason: format!("Failed to fetch spaces from {}: {}", server_url, e),
        })?;

    if !resp.status().is_success() {
        return Err(ExtensionError::ValidationError {
            reason: format!(
                "Failed to list spaces from {}: HTTP {}",
                server_url,
                resp.status()
            ),
        });
    }

    resp.json()
        .await
        .map_err(|e| ExtensionError::ValidationError {
            reason: format!("Invalid spaces response: {}", e),
        })
}

/// List all shared spaces the user is a member of (with decrypted names).
///
/// Uses the JWT from AppState.auth_token (synced from frontend Supabase session).
#[tauri::command]
pub async fn extension_space_list(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<Vec<DecryptedSpace>, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let perm_result =
        PermissionManager::check_spaces_permission(&state, &extension_id, SpaceAction::Read)
            .await;
    if let Err(ref e) = perm_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    perm_result?;

    let token = get_auth_token(&state)?;

    // Get unique server URLs from enabled backends
    let backend_rows = core::select_with_crdt(
        "SELECT id, server_url, enabled FROM haex_sync_backends".to_string(),
        vec![],
        &state.db,
    )
    .map_err(|e| ExtensionError::Database {
        source: DatabaseError::DatabaseError {
            reason: e.to_string(),
        },
    })?;

    let mut seen_urls = std::collections::HashSet::new();
    let mut server_urls: Vec<String> = Vec::new();
    for row in &backend_rows {
        let enabled = get_bool(row, 2);
        if enabled {
            let server_url = get_string(row, 1);
            if !server_url.is_empty() && seen_urls.insert(server_url.clone()) {
                server_urls.push(server_url);
            }
        }
    }

    let client = reqwest::Client::new();
    let mut all_spaces: Vec<DecryptedSpace> = Vec::new();

    for server_url in &server_urls {
        let server_spaces = match fetch_spaces(&client, server_url, &token).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[SpaceList] Failed to fetch spaces from {}: {}", server_url, e);
                continue;
            }
        };

        for space in server_spaces {
            let decrypted_name =
                match get_space_key(&state.db, &space.id, space.current_key_generation) {
                    Some(key_base64) => {
                        decrypt_space_name(&key_base64, &space.encrypted_name, &space.name_nonce)
                            .unwrap_or_else(|_| {
                                format!("Space {}", &space.id[..8.min(space.id.len())])
                            })
                    }
                    None => format!("Space {}", &space.id[..8.min(space.id.len())]),
                };

            all_spaces.push(DecryptedSpace {
                id: space.id,
                name: decrypted_name,
                role: space.role,
                server_url: server_url.clone(),
                created_at: space.created_at,
            });
        }
    }

    Ok(all_spaces)
}

/// Create a new shared space on a sync backend.
///
/// Uses the JWT from AppState.auth_token (synced from frontend Supabase session).
#[tauri::command]
pub async fn extension_space_create(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    space_name: String,
    server_url: String,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<DecryptedSpace, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let perm_result =
        PermissionManager::check_spaces_permission(&state, &extension_id, SpaceAction::ReadWrite)
            .await;
    if let Err(ref e) = perm_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    perm_result?;

    let token = get_auth_token(&state)?;

    // Get user's public key from local identity linked to this backend
    let user_public_key_base64 = get_identity_public_key(&state.db, &server_url)?;

    let client = reqwest::Client::new();

    // Generate space key (32 random bytes)
    let mut space_key = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut space_key);

    // Encrypt space name with space key (AES-256-GCM)
    let (encrypted_name, name_nonce) = encrypt_space_name_raw(&space_key, &space_name)?;

    // Encrypt space key for self (ECDH + HKDF + AES-GCM)
    let key_grant = encrypt_space_key_for_recipient(&space_key, &user_public_key_base64)?;

    let space_id = uuid::Uuid::new_v4().to_string();

    let url = format!("{}/spaces", server_url.trim_end_matches('/'));
    let body = json!({
        "id": space_id,
        "encryptedName": encrypted_name,
        "nameNonce": name_nonce,
        "label": space_name,
        "keyGrant": {
            "encryptedSpaceKey": key_grant.encrypted_space_key,
            "keyNonce": key_grant.key_nonce,
            "ephemeralPublicKey": key_grant.ephemeral_public_key,
        }
    });

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&body)
        .send()
        .await
        .map_err(|e| ExtensionError::ValidationError {
            reason: format!("Failed to create space: {}", e),
        })?;

    if !resp.status().is_success() {
        let error_text = resp.text().await.unwrap_or_default();
        return Err(ExtensionError::ValidationError {
            reason: format!("Server rejected space creation: {}", error_text),
        });
    }

    persist_space_key(&state.db, &space_id, 1, &space_key)?;

    Ok(DecryptedSpace {
        id: space_id,
        name: space_name,
        role: "admin".to_string(),
        server_url,
        created_at: {
            let now = time::OffsetDateTime::now_utc();
            now.format(&time::format_description::well_known::Rfc3339).unwrap_or_default()
        },
    })
}

// ============================================================================
// Space Key Management (local DB)
// ============================================================================

/// Look up a space key from the local database.
fn get_space_key(db: &DbConnection, space_id: &str, generation: i64) -> Option<String> {
    with_connection(db, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT key FROM haex_space_keys_no_sync \
                 WHERE space_id = ?1 AND generation = ?2 LIMIT 1",
            )
            .map_err(DatabaseError::from)?;
        let mut rows = stmt
            .query(rusqlite::params![space_id, generation])
            .map_err(DatabaseError::from)?;
        if let Some(row) = rows.next().map_err(DatabaseError::from)? {
            let key: String = row.get(0).map_err(DatabaseError::from)?;
            Ok(Some(key))
        } else {
            Ok(None)
        }
    })
    .ok()
    .flatten()
}

/// Persist a space key to the local database.
fn persist_space_key(
    db: &DbConnection,
    space_id: &str,
    generation: i64,
    key: &[u8],
) -> Result<(), ExtensionError> {
    let key_base64 = BASE64.encode(key);
    with_connection(db, |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO haex_space_keys_no_sync (space_id, generation, key) \
             VALUES (?1, ?2, ?3)",
            rusqlite::params![space_id, generation, key_base64],
        )
        .map_err(DatabaseError::from)?;
        Ok(())
    })?;
    Ok(())
}

// ============================================================================
// Identity Lookup (local DB)
// ============================================================================

/// Look up user's public key from the local database by finding the identity
/// linked to the sync backend for the given server URL.
fn get_identity_public_key(
    db: &DbConnection,
    server_url: &str,
) -> Result<String, ExtensionError> {
    with_connection(db, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT i.public_key FROM haex_identities i \
                 INNER JOIN haex_sync_backends b ON b.identity_id = i.id \
                 WHERE b.server_url = ?1 LIMIT 1",
            )
            .map_err(DatabaseError::from)?;
        let mut rows = stmt
            .query(rusqlite::params![server_url])
            .map_err(DatabaseError::from)?;
        if let Some(row) = rows.next().map_err(DatabaseError::from)? {
            let public_key: String = row.get(0).map_err(DatabaseError::from)?;
            Ok(public_key)
        } else {
            Err(DatabaseError::ExecutionError {
                sql: "identity lookup by server_url".to_string(),
                reason: format!("No identity linked to backend {}", server_url),
                table: None,
            })
        }
    })
    .map_err(|e| ExtensionError::ValidationError {
        reason: format!("No identity found for server: {}", e),
    })
}

// ============================================================================
// Crypto Helpers
// ============================================================================

/// Encrypt a space name with AES-256-GCM using the raw space key.
fn encrypt_space_name_raw(
    space_key: &[u8; 32],
    name: &str,
) -> Result<(String, String), ExtensionError> {
    let cipher = Aes256Gcm::new_from_slice(space_key).map_err(|e| {
        ExtensionError::ValidationError {
            reason: format!("Invalid AES key: {}", e),
        }
    })?;

    let mut nonce_bytes = [0u8; 12];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let encrypted = cipher
        .encrypt(nonce, name.as_bytes())
        .map_err(|e| ExtensionError::ValidationError {
            reason: format!("Encryption failed: {}", e),
        })?;

    Ok((BASE64.encode(&encrypted), BASE64.encode(&nonce_bytes)))
}

/// Result of encrypting a space key for a recipient.
struct EncryptedKeyGrant {
    encrypted_space_key: String,
    key_nonce: String,
    ephemeral_public_key: String,
}

/// Encrypt a space key for a recipient using ECDH (P-256) + HKDF + AES-256-GCM.
fn encrypt_space_key_for_recipient(
    space_key: &[u8; 32],
    recipient_public_key_base64: &str,
) -> Result<EncryptedKeyGrant, ExtensionError> {
    use p256::ecdh::EphemeralSecret;
    use p256::PublicKey;

    let recipient_spki = BASE64
        .decode(recipient_public_key_base64)
        .map_err(|e| ExtensionError::ValidationError {
            reason: format!("Invalid recipient public key: {}", e),
        })?;

    let recipient_key =
        PublicKey::from_public_key_der(&recipient_spki).map_err(|e| {
            ExtensionError::ValidationError {
                reason: format!("Failed to parse recipient public key: {}", e),
            }
        })?;

    let ephemeral_secret = EphemeralSecret::random(&mut rand::rngs::OsRng);
    let ephemeral_public = ephemeral_secret.public_key();

    let shared_secret = ephemeral_secret.diffie_hellman(&recipient_key);
    let shared_bits = shared_secret.raw_secret_bytes();

    let hk = hkdf::Hkdf::<sha2::Sha256>::new(Some(&[]), shared_bits.as_slice());
    let mut aes_key_bytes = [0u8; 32];
    hk.expand(b"haex-space-key", &mut aes_key_bytes)
        .map_err(|e| ExtensionError::ValidationError {
            reason: format!("HKDF expand failed: {}", e),
        })?;

    let cipher = Aes256Gcm::new_from_slice(&aes_key_bytes).map_err(|e| {
        ExtensionError::ValidationError {
            reason: format!("Invalid derived AES key: {}", e),
        }
    })?;
    let mut nonce_bytes = [0u8; 12];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let encrypted = cipher
        .encrypt(nonce, space_key.as_slice())
        .map_err(|e| ExtensionError::ValidationError {
            reason: format!("Space key encryption failed: {}", e),
        })?;

    let eph_spki = ephemeral_public
        .to_public_key_der()
        .map_err(|e| ExtensionError::ValidationError {
            reason: format!("Failed to export ephemeral public key: {}", e),
        })?;

    Ok(EncryptedKeyGrant {
        encrypted_space_key: BASE64.encode(&encrypted),
        key_nonce: BASE64.encode(&nonce_bytes),
        ephemeral_public_key: BASE64.encode(eph_spki.as_bytes()),
    })
}

/// Decrypt a space name using a locally stored space key.
fn decrypt_space_name(
    space_key_base64: &str,
    encrypted_name_base64: &str,
    nonce_base64: &str,
) -> Result<String, ExtensionError> {
    let key_bytes = BASE64.decode(space_key_base64).map_err(|e| {
        ExtensionError::ValidationError {
            reason: format!("Invalid space key: {}", e),
        }
    })?;

    let encrypted = BASE64
        .decode(encrypted_name_base64)
        .map_err(|e| ExtensionError::ValidationError {
            reason: format!("Invalid encrypted name: {}", e),
        })?;

    let nonce_bytes = BASE64
        .decode(nonce_base64)
        .map_err(|e| ExtensionError::ValidationError {
            reason: format!("Invalid nonce: {}", e),
        })?;

    let cipher = Aes256Gcm::new_from_slice(&key_bytes).map_err(|e| {
        ExtensionError::ValidationError {
            reason: format!("Invalid AES key: {}", e),
        }
    })?;

    let nonce = Nonce::from_slice(&nonce_bytes);
    let decrypted = cipher.decrypt(nonce, encrypted.as_ref()).map_err(|e| {
        ExtensionError::ValidationError {
            reason: format!("Decryption failed: {}", e),
        }
    })?;

    String::from_utf8(decrypted).map_err(|e| ExtensionError::ValidationError {
        reason: format!("Decrypted name is not valid UTF-8: {}", e),
    })
}
