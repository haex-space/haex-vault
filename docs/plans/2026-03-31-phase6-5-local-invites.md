# Phase 6.5: Invite Flow for Local Spaces

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Enable inviting members to local spaces via direct contact invite or open "conference" invite link. The invite flow happens over QUIC in a single handshake — no server intermediary.

**Architecture:** Two invite modes, same QUIC endpoint:

1. **Contact Invite (targeted):** Admin knows the invitee's DID. Invite is bound to that DID. Only that person can claim it.
2. **Conference Invite (open):** Admin creates a token with `maxUses` + `expiresAt`. Anyone with the link can claim it until limits are reached.

Both modes: Invitee connects to leader via QUIC → sends token + KeyPackages → Leader validates → creates UCAN delegation → `mls_add_member` → sends Welcome back → Invitee is a member. All in one roundtrip.

**Invite link format:**
```
haexvault://invite/local?endpoint={leaderEndpointId}&space={spaceId}&token={inviteToken}&relay={optionalRelay}
```

**Tech Stack:** Rust (protocol types, leader invite handler, token storage), TypeScript (invite link parsing, UI integration)

---

### Task 1: Implement UCAN creation in Rust

The leader needs to create UCAN delegation tokens in Rust. The UCAN format is simple: `base64url(header).base64url(payload).base64url(ed25519_signature)`.

**Files:**
- Create: `src-tauri/src/space_delivery/local/ucan.rs`
- Modify: `src-tauri/src/space_delivery/local/mod.rs` (add `pub mod ucan;`)

**Implementation:**

```rust
//! UCAN token creation in Rust for local space invites.
//!
//! Mirrors the TypeScript @haex-space/ucan library format:
//! Header: { "alg": "EdDSA", "typ": "JWT" }
//! Payload: { "ucv": "1.0", "iss": did, "aud": did, "cap": {...}, "exp": ..., "iat": ..., "prf": [...], "nnc": ... }
//! Signature: Ed25519 over `base64url(header).base64url(payload)`

use ed25519_dalek::{SigningKey, Signer};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::json;

/// Create a delegated UCAN token.
/// `issuer_private_key_pkcs8` is the Base64-encoded PKCS8 private key from haex_identities.
pub fn create_delegated_ucan(
    issuer_did: &str,
    issuer_private_key_pkcs8: &str,
    audience_did: &str,
    space_id: &str,
    capability: &str,
    parent_ucan: Option<&str>,
    expires_in_seconds: u64,
) -> Result<String, String>
```

**Logic:**
1. Decode PKCS8 private key from Base64 → extract raw 32-byte Ed25519 seed
2. Build header JSON: `{"alg":"EdDSA","typ":"JWT"}`
3. Build payload JSON with `ucv`, `iss`, `aud`, `cap`, `exp`, `iat`, `prf`, `nnc`
4. Encode both as base64url
5. Sign `header.payload` with Ed25519
6. Return `header.payload.signature`

The capability format: `{ "space:{spaceId}": "{capability}" }`

For the nonce: generate 12 random bytes, base64url-encode.

**Note on PKCS8:** The identity private keys in the DB are PKCS8-encoded (not raw 32 bytes). PKCS8 for Ed25519 has a fixed prefix — the raw key starts at byte offset 16. Or use `ed25519_dalek::pkcs8::DecodePrivateKey` if available.

**Also add a helper to load the admin identity:**

```rust
/// Load the admin's identity (DID + private key) from the database for a space.
pub fn load_admin_identity_for_space(
    db: &DbConnection,
    space_id: &str,
) -> Result<(String, String), String>  // (did, private_key_base64)
```

This queries: get the root UCAN for the space from `haex_ucan_tokens` → extract issuer DID → look up in `haex_identities` → return DID + private key.

Or simpler: Get the first identity that has a UCAN for this space.

**Verification:** `cargo check`

**Commit:**
```
feat: implement UCAN token creation in Rust
```

---

### Task 2: Add invite protocol types

Extend the delivery protocol with invite-specific request/response types.

**Files:**
- Modify: `src-tauri/src/space_delivery/local/protocol.rs`

**Add to the `Request` enum:**

```rust
// -- Invites --
/// Claim an invite token. Invitee sends token + KeyPackages, leader validates and adds to MLS group.
ClaimInvite {
    space_id: String,
    /// The invite token ID
    token: String,
    /// Invitee's DID
    did: String,
    /// Invitee's endpoint ID
    endpoint_id: String,
    /// Base64-encoded MLS KeyPackages
    key_packages: Vec<String>,
    /// Optional: label and claims to share
    label: Option<String>,
},
```

**Add to the `Response` enum:**

```rust
/// Invite claimed successfully — includes MLS welcome and delegated UCAN
InviteClaimed {
    /// Base64-encoded MLS welcome message
    welcome: String,
    /// The delegated UCAN token for this member
    ucan: String,
    /// The capability granted
    capability: String,
},
```

**Verification:** `cargo check`

**Commit:**
```
feat: add invite protocol types for local spaces
```

---

### Task 2: Add local invite token storage in leader

The leader stores invite tokens in a `_no_sync` table. We already have the Drizzle schema for buffer tables, but we need a new table for invite tokens.

Since we don't want to regenerate migrations mid-feature, store invite tokens in-memory on the leader (they're ephemeral — if leader restarts, tokens are lost, admin creates new ones).

**Files:**
- Modify: `src-tauri/src/space_delivery/local/leader.rs`

**Add to `LeaderState`:**

```rust
/// Active invite tokens (in-memory, lost on restart)
pub invite_tokens: Arc<RwLock<Vec<LocalInviteToken>>>,
```

**Add struct:**

```rust
/// A local invite token created by the admin.
#[derive(Debug, Clone)]
pub struct LocalInviteToken {
    pub id: String,
    pub space_id: String,
    /// If Some, only this DID can claim (contact invite). If None, anyone can claim (conference).
    pub target_did: Option<String>,
    pub capability: String,
    pub max_uses: u32,
    pub current_uses: u32,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl LocalInviteToken {
    pub fn is_valid(&self) -> bool {
        self.current_uses < self.max_uses && chrono::Utc::now() < self.expires_at
    }

    pub fn can_claim(&self, did: &str) -> bool {
        self.is_valid() && self.target_did.as_ref().map_or(true, |t| t == did)
    }
}
```

**Add functions:**

```rust
/// Create an invite token (admin-side).
pub fn create_invite_token(
    state: &LeaderState,
    target_did: Option<String>,
    capability: String,
    max_uses: u32,
    expires_in_seconds: u64,
) -> String  // returns token ID

/// Validate and consume an invite token (returns capability if valid).
pub async fn validate_and_consume_invite(
    state: &LeaderState,
    token_id: &str,
    claimer_did: &str,
) -> Result<String, DeliveryError>  // returns capability
```

Initialize `invite_tokens: Arc::new(RwLock::new(Vec::new()))` in LeaderState construction (in commands.rs `local_delivery_start`).

**Verification:** `cargo check`

**Commit:**
```
feat: add local invite token management in leader
```

---

### Task 3: Handle ClaimInvite in leader connection handler

When a peer sends `ClaimInvite`, the leader does the full MLS add-member flow.

**Files:**
- Modify: `src-tauri/src/space_delivery/local/leader.rs` (in `handle_delivery_stream`)

**Add the ClaimInvite handler in the request match:**

```rust
Request::ClaimInvite { space_id, token, did, endpoint_id, key_packages, label } => {
    if space_id != state.space_id {
        Response::Error { message: format!("Wrong space: expected {}", state.space_id) }
    } else {
        // 1. Validate and consume invite token
        let capability = match validate_and_consume_invite(&state, &token, &did).await {
            Ok(cap) => cap,
            Err(e) => return Ok(send_response(&mut send, Response::Error { message: e.to_string() }).await?),
        };

        // 2. Store key packages from invitee
        for pkg_b64 in &key_packages {
            if let Ok(blob) = base64_decode(pkg_b64) {
                let _ = store_key_package(&state.db, &space_id, &did, &blob);
            }
        }

        // 3. Fetch one key package back for mls_add_member
        let key_package_blob = match consume_key_package(&state.db, &space_id, &did)? {
            Some(blob) => blob,
            None => return Ok(send_response(&mut send, Response::Error {
                message: "No key package available after upload".into()
            }).await?),
        };

        // 4. MLS add_member (leader has MLS group)
        let mls_manager = crate::mls::manager::MlsManager::new(state.db.0.clone());
        let bundle = mls_manager.add_member(&space_id, &key_package_blob)
            .map_err(|e| DeliveryError::Mls { reason: e })?;

        // 5. Create delegated UCAN for the new member
        //    For now, return capability string — frontend creates UCAN on admin side
        //    (UCAN creation requires identity private key which is in frontend)

        // 6. Send commit to all existing members
        if let Some(commit) = &bundle.commit {
            let msg_id = store_message(&state.db, &space_id, "leader", "commit", commit)?;
            let senders = state.notification_senders.read().await;
            for (_, sender) in senders.iter() {
                let _ = sender.try_send(Notification::Mls {
                    space_id: space_id.clone(),
                    message_type: "commit".to_string(),
                });
            }
        }

        // 7. Register peer as connected
        let peer = ConnectedPeer {
            endpoint_id: endpoint_id.clone(),
            did: did.clone(),
            label,
            claims: vec![],
            connected_at: chrono::Utc::now().to_rfc3339(),
        };
        state.connected_peers.write().await.insert(endpoint_id, peer);

        // 8. Return welcome + capability
        match bundle.welcome {
            Some(welcome) => Response::InviteClaimed {
                welcome: base64_encode(&welcome),
                ucan: String::new(), // TODO: UCAN delegation happens on frontend for now
                capability,
            },
            None => Response::Error { message: "MLS add_member produced no welcome".into() },
        }
    }
}
```

NOTE: The MLS manager's `add_member` method needs to be checked — read `src-tauri/src/mls/manager.rs` to see the exact signature and what it returns (the `MlsCommitBundle` struct fields: `commit`, `welcome`, `group_info`). Adapt accordingly.

**Verification:** `cargo check`

**Commit:**
```
feat: handle ClaimInvite in leader for local MLS onboarding
```

---

### Task 4: Add Tauri commands for local invite management

**Files:**
- Modify: `src-tauri/src/space_delivery/local/commands.rs`
- Modify: `src-tauri/src/lib.rs` (register commands)

**New commands:**

```rust
/// Create a local invite token (admin-side).
#[tauri::command]
pub async fn local_delivery_create_invite(
    state: State<'_, AppState>,
    space_id: String,
    target_did: Option<String>,
    capability: String,
    max_uses: u32,
    expires_in_seconds: u64,
) -> Result<String, String>  // returns token ID

/// List active invite tokens for a space.
#[tauri::command]
pub async fn local_delivery_list_invites(
    state: State<'_, AppState>,
    space_id: String,
) -> Result<Vec<LocalInviteInfo>, String>

/// Revoke an invite token.
#[tauri::command]
pub async fn local_delivery_revoke_invite(
    state: State<'_, AppState>,
    token_id: String,
) -> Result<(), String>

/// Claim a local invite as a peer (invitee-side).
/// Connects to leader, sends token + KeyPackages, receives Welcome.
#[tauri::command]
pub async fn local_delivery_claim_invite(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    leader_endpoint_id: String,
    leader_relay_url: Option<String>,
    space_id: String,
    token_id: String,
    identity_did: String,
    label: Option<String>,
) -> Result<ClaimInviteResult, String>
```

`local_delivery_create_invite`: Gets LeaderState from delivery_handler, calls `create_invite_token`.

`local_delivery_claim_invite`:
1. Get iroh endpoint + generate KeyPackages
2. Connect to leader via QUIC
3. Send `ClaimInvite` request
4. On `InviteClaimed` response: process MLS welcome, persist space locally, start sync loop
5. Return result to frontend

**Add to types.rs:**

```rust
#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LocalInviteInfo {
    pub id: String,
    pub target_did: Option<String>,
    pub capability: String,
    pub max_uses: u32,
    pub current_uses: u32,
    pub expires_at: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ClaimInviteResult {
    pub space_id: String,
    pub capability: String,
}
```

Register all 4 new commands in lib.rs.

**Verification:** `cargo check`

**Commit:**
```
feat: add Tauri commands for local invite management
```

---

### Task 5: Extend invite link parsing for local invites

**Files:**
- Modify: `src/utils/inviteLink.ts`

**Add local invite link support:**

```typescript
export interface LocalInviteLink {
  endpointId: string
  spaceId: string
  tokenId: string
  relayUrl?: string
}

const LOCAL_INVITE_PREFIX = 'haexvault://invite/local'

export function isLocalInviteLink(str: string): boolean {
  return str.startsWith(LOCAL_INVITE_PREFIX)
}

export function parseLocalInviteLink(link: string): LocalInviteLink | null {
  try {
    const url = new URL(link)
    const endpointId = url.searchParams.get('endpoint')
    const spaceId = url.searchParams.get('space')
    const tokenId = url.searchParams.get('token')
    const relayUrl = url.searchParams.get('relay') || undefined

    if (!endpointId || !spaceId || !tokenId) return null
    return { endpointId, spaceId, tokenId, relayUrl }
  } catch {
    return null
  }
}

export function buildLocalInviteLink(
  endpointId: string,
  spaceId: string,
  tokenId: string,
  relayUrl?: string,
): string {
  const params = new URLSearchParams({ endpoint: endpointId, space: spaceId, token: tokenId })
  if (relayUrl) params.set('relay', relayUrl)
  return `${LOCAL_INVITE_PREFIX}?${params.toString()}`
}
```

Update `isInviteLink` to also check local prefix:
```typescript
export function isInviteLink(str: string): boolean {
  return str.startsWith(WEB_INVITE_PREFIX) || str.startsWith(APP_INVITE_PREFIX) || str.startsWith(LOCAL_INVITE_PREFIX)
}
```

**Verification:** `npx vue-tsc --noEmit`

**Commit:**
```
feat: add local invite link parsing and generation
```

---

### Task 6: Wire local invites into the UI

**Files:**
- Modify: `src/components/haex/system/settings/spaces.vue` (join dialog handles local links)
- Modify: `src/stores/spaces.ts` (add local invite store functions)

**In spaces.vue `onJoinSpaceAsync`:**

Add a check for local invite links before the existing server invite logic:

```typescript
// Check if it's a local invite
const localLink = parseLocalInviteLink(joinInviteLink.value.trim())
if (localLink) {
  const identityId = identityStore.identities[0]?.publicKey
  if (!identityId) {
    add({ title: t('errors.noIdentity'), color: 'error' })
    return
  }
  const identity = await identityStore.getIdentityAsync(identityId)

  const result = await invoke<{ spaceId: string; capability: string }>('local_delivery_claim_invite', {
    leaderEndpointId: localLink.endpointId,
    leaderRelayUrl: localLink.relayUrl || null,
    spaceId: localLink.spaceId,
    tokenId: localLink.tokenId,
    identityDid: identity.did,
    label: identity.label || null,
  })

  add({ title: t('success.joined'), color: 'success' })
  showJoinDialog.value = false
  joinInviteLink.value = ''
  await spacesStore.loadSpacesFromDbAsync()
  return
}
```

Import `parseLocalInviteLink` from `~/utils/inviteLink`.

**In SpaceInviteDialog.vue:**

For local spaces (no serverUrl), the invite dialog should:
- Call `local_delivery_create_invite` instead of `createInviteTokenAsync`
- Build link with `buildLocalInviteLink` instead of `buildInviteLink`
- Get the leader's endpoint ID and relay URL for the link

This requires knowing the leader's endpoint ID. Add it to the dialog props or fetch it:
```typescript
const endpointId = await invoke<string>('peer_storage_status').then(s => s.nodeId)
const relayUrl = // from vault settings
```

**Verification:** `npx vue-tsc --noEmit`

**Commit:**
```
feat: wire local invites into spaces UI
```

---

### Task 7: Verify full build

**Step 1:** `cargo check`
**Step 2:** `npx vue-tsc --noEmit`

---

## Summary

| File | Change |
|------|--------|
| `protocol.rs` | `ClaimInvite` request + `InviteClaimed` response |
| `leader.rs` | `LocalInviteToken` struct, token management, `ClaimInvite` handler with MLS |
| `commands.rs` | 4 new commands: create/list/revoke/claim invite |
| `types.rs` | `LocalInviteInfo`, `ClaimInviteResult` (TS-exported) |
| `inviteLink.ts` | Local invite link parsing + generation |
| `spaces.vue` | Join dialog handles local invite links |
| `SpaceInviteDialog.vue` | Creates local invites for local spaces |
| `lib.rs` | Register new commands |
