//! Tauri commands for extension shared space management.
//!
//! These commands allow extensions to:
//! - Assign/unassign their table rows to shared spaces for selective sync
//! - List available sync backends
//! - List shared spaces the user is a member of
//! - Create new shared spaces

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
    // Optional parameters for iframe mode
    public_key: Option<String>,
    name: Option<String>,
) -> Result<u64, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    // Check spaces:readWrite permission
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
    // Optional parameters for iframe mode
    public_key: Option<String>,
    name: Option<String>,
) -> Result<u64, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    // Check spaces:readWrite permission
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
    // Optional parameters for iframe mode
    public_key: Option<String>,
    name: Option<String>,
) -> Result<Vec<SpaceAssignmentRow>, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    // Check spaces:read permission
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
                // Build a query with IN clause using positional parameters
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
                // No filter, return all assignments for this table
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
    pub can_invite: bool,
    pub server_url: String,
    pub created_at: String,
}

/// List available sync backends that can host shared spaces.
///
/// Returns personal (non-space) enabled backends with their server URLs.
/// The backend with highest priority is marked as default.
#[tauri::command]
pub async fn extension_space_list_backends(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<Vec<SyncBackendInfo>, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    // Check spaces:read permission
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

    // Sort by priority descending, highest is default
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

/// Backend credentials extracted from SQLite.
struct BackendCredentials {
    server_url: String,
    email: String,
    password: String,
}

/// Login response from the sync server's POST /auth/login endpoint.
#[derive(Deserialize)]
struct LoginResponse {
    access_token: String,
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
    can_invite: bool,
    created_at: String,
}

/// Authenticate with a sync server and return an access token.
async fn login_to_server(
    client: &reqwest::Client,
    server_url: &str,
    email: &str,
    password: &str,
) -> Result<String, ExtensionError> {
    let url = format!("{}/auth/login", server_url.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .json(&json!({ "email": email, "password": password }))
        .send()
        .await
        .map_err(|e| ExtensionError::ValidationError {
            reason: format!("HTTP request to {} failed: {}", url, e),
        })?;

    if !resp.status().is_success() {
        return Err(ExtensionError::ValidationError {
            reason: format!("Login failed for {}: HTTP {}", server_url, resp.status()),
        });
    }

    let login: LoginResponse =
        resp.json().await.map_err(|e| ExtensionError::ValidationError {
            reason: format!("Invalid login response: {}", e),
        })?;

    Ok(login.access_token)
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
/// For each sync backend, authenticates with the server, fetches spaces,
/// and decrypts their names using locally stored space keys.
#[tauri::command]
pub async fn extension_space_list(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<Vec<DecryptedSpace>, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    // Check spaces:read permission
    let perm_result =
        PermissionManager::check_spaces_permission(&state, &extension_id, SpaceAction::Read)
            .await;
    if let Err(ref e) = perm_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    perm_result?;

    // Get enabled backends with credentials
    let backend_rows = core::select_with_crdt(
        "SELECT id, server_url, email, password, enabled FROM haex_sync_backends".to_string(),
        vec![],
        &state.db,
    )
    .map_err(|e| ExtensionError::Database {
        source: DatabaseError::DatabaseError {
            reason: e.to_string(),
        },
    })?;

    // Collect unique server credentials (deduplicate by server_url)
    let mut seen_urls = std::collections::HashSet::new();
    let mut credentials: Vec<BackendCredentials> = Vec::new();
    for row in &backend_rows {
        let enabled = get_bool(row, 4);
        if enabled {
            let server_url = get_string(row, 1);
            if seen_urls.insert(server_url.clone()) {
                credentials.push(BackendCredentials {
                    server_url,
                    email: get_string(row, 2),
                    password: get_string(row, 3),
                });
            }
        }
    }

    let client = reqwest::Client::new();
    let mut all_spaces: Vec<DecryptedSpace> = Vec::new();

    for cred in &credentials {
        // Authenticate
        let token = match login_to_server(&client, &cred.server_url, &cred.email, &cred.password)
            .await
        {
            Ok(t) => t,
            Err(e) => {
                eprintln!(
                    "[SpaceList] Auth failed for {}: {}",
                    cred.server_url, e
                );
                continue;
            }
        };

        // Fetch spaces
        let server_spaces = match fetch_spaces(&client, &cred.server_url, &token).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "[SpaceList] Failed to fetch spaces from {}: {}",
                    cred.server_url, e
                );
                continue;
            }
        };

        // Decrypt names using locally stored space keys
        for space in server_spaces {
            let decrypted_name = match get_space_key(&state.db, &space.id, space.current_key_generation) {
                Some(key_base64) => {
                    decrypt_space_name(&key_base64, &space.encrypted_name, &space.name_nonce)
                        .unwrap_or_else(|_| format!("Space {}", &space.id[..8.min(space.id.len())]))
                }
                None => format!("Space {}", &space.id[..8.min(space.id.len())]),
            };

            all_spaces.push(DecryptedSpace {
                id: space.id,
                name: decrypted_name,
                role: space.role,
                can_invite: space.can_invite,
                server_url: cred.server_url.clone(),
                created_at: space.created_at,
            });
        }
    }

    Ok(all_spaces)
}

/// Create a new shared space on a sync backend.
///
/// Generates a space key, encrypts the space name, encrypts the space key
/// for the user (ECDH + HKDF + AES-GCM), creates the space on the server,
/// and persists the space key locally.
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

    // Check spaces:readWrite permission
    let perm_result =
        PermissionManager::check_spaces_permission(&state, &extension_id, SpaceAction::ReadWrite)
            .await;
    if let Err(ref e) = perm_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    perm_result?;

    // Find credentials for this server
    let backend_rows = core::select_with_crdt(
        "SELECT id, server_url, email, password, enabled FROM haex_sync_backends".to_string(),
        vec![],
        &state.db,
    )
    .map_err(|e| ExtensionError::Database {
        source: DatabaseError::DatabaseError {
            reason: e.to_string(),
        },
    })?;

    let cred = backend_rows
        .iter()
        .find(|row| {
            let url = get_string(row, 1);
            let enabled = get_bool(row, 4);
            url == server_url && enabled
        })
        .map(|row| BackendCredentials {
            server_url: get_string(row, 1),
            email: get_string(row, 2),
            password: get_string(row, 3),
        })
        .ok_or_else(|| ExtensionError::ValidationError {
            reason: format!("No backend found for server URL: {}", server_url),
        })?;

    let client = reqwest::Client::new();

    // Authenticate
    let token = login_to_server(&client, &cred.server_url, &cred.email, &cred.password).await?;

    // Get user's public key from server
    let user_public_key_base64 = fetch_user_public_key(&client, &cred.server_url, &token).await?;

    // Generate space key (32 random bytes)
    let mut space_key = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut space_key);

    // Encrypt space name with space key (AES-256-GCM)
    let (encrypted_name, name_nonce) = encrypt_space_name_raw(&space_key, &space_name)?;

    // Encrypt space key for self (ECDH + HKDF + AES-GCM)
    let key_grant = encrypt_space_key_for_recipient(&space_key, &user_public_key_base64)?;

    // Generate space ID
    let space_id = uuid::Uuid::new_v4().to_string();

    // POST to server
    let url = format!("{}/spaces", cred.server_url.trim_end_matches('/'));
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

    // Persist space key locally
    persist_space_key(&state.db, &space_id, 1, &space_key)?;

    Ok(DecryptedSpace {
        id: space_id,
        name: space_name,
        role: "admin".to_string(),
        can_invite: true,
        server_url,
        created_at: chrono_now_iso(),
    })
}

/// Look up a space key from the local database (non-CRDT table).
fn get_space_key(
    db: &DbConnection,
    space_id: &str,
    generation: i64,
) -> Option<String> {
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

/// Fetch user's public key from the server.
async fn fetch_user_public_key(
    client: &reqwest::Client,
    server_url: &str,
    token: &str,
) -> Result<String, ExtensionError> {
    let url = format!("{}/keypairs/me", server_url.trim_end_matches('/'));
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| ExtensionError::ValidationError {
            reason: format!("Failed to fetch keypair: {}", e),
        })?;

    if !resp.status().is_success() {
        return Err(ExtensionError::ValidationError {
            reason: "No keypair registered on server".to_string(),
        });
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct KeypairResponse {
        public_key: String,
    }

    let data: KeypairResponse =
        resp.json().await.map_err(|e| ExtensionError::ValidationError {
            reason: format!("Invalid keypair response: {}", e),
        })?;

    Ok(data.public_key)
}

/// Encrypt a space name with AES-256-GCM using the raw space key.
/// Returns (encrypted_name_base64, nonce_base64).
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
/// Matches the SDK's `encryptSpaceKeyForRecipientAsync` exactly.
fn encrypt_space_key_for_recipient(
    space_key: &[u8; 32],
    recipient_public_key_base64: &str,
) -> Result<EncryptedKeyGrant, ExtensionError> {
    use p256::ecdh::EphemeralSecret;
    use p256::PublicKey;

    // Decode recipient's SPKI public key
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

    // Generate ephemeral ECDH keypair
    let ephemeral_secret = EphemeralSecret::random(&mut rand::rngs::OsRng);
    let ephemeral_public = ephemeral_secret.public_key();

    // ECDH: derive shared secret
    let shared_secret = ephemeral_secret.diffie_hellman(&recipient_key);
    let shared_bits = shared_secret.raw_secret_bytes();

    // HKDF(SHA-256, salt=empty, info="haex-space-key") → 32-byte AES key
    let hk = hkdf::Hkdf::<sha2::Sha256>::new(Some(&[]), shared_bits.as_slice());
    let mut aes_key_bytes = [0u8; 32];
    hk.expand(b"haex-space-key", &mut aes_key_bytes)
        .map_err(|e| ExtensionError::ValidationError {
            reason: format!("HKDF expand failed: {}", e),
        })?;

    // AES-256-GCM encrypt the space key
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

    // Export ephemeral public key as SPKI DER → base64
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

/// Return current time as ISO 8601 string.
fn chrono_now_iso() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Simple ISO format: YYYY-MM-DDTHH:MM:SSZ
    let secs_per_day = 86400u64;
    let days = now / secs_per_day;
    let time_of_day = now % secs_per_day;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Calculate date from days since epoch (1970-01-01)
    let mut y = 1970i64;
    let mut remaining_days = days as i64;
    loop {
        let days_in_year = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
            366
        } else {
            365
        };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        y += 1;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut m = 0usize;
    for &md in &month_days {
        if remaining_days < md {
            break;
        }
        remaining_days -= md;
        m += 1;
    }
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        m + 1,
        remaining_days + 1,
        hours,
        minutes,
        seconds
    )
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
