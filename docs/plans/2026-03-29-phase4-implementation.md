# Phase 4: Client-Integration + Realtime Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Migrate haex-vault client to UCAN/DID-Auth, replace Supabase Realtime with own WebSocket, refactor Spaces Store.

**Architecture:** Server gets a WebSocket endpoint for push notifications. Client gets DID-Auth and UCAN header builders, a WebSocket composable, and refactored stores that use the new auth. Supabase stays only for GoTrue (email) and as PostgreSQL host.

**Tech Stack:** Vue 3/Nuxt UI, Pinia stores, `@haex-space/ucan`, `@haex-space/vault-sdk`, Hono WebSocket (Bun), Tauri (Rust), Drizzle ORM

**Design doc:** `docs/plans/2026-03-29-phase4-client-integration.md`

---

## Dependency Graph

```
Task 1 (Server: WebSocket endpoint)
Task 2 (Server: Push triggers WS events)  ← needs Task 1
    ↓
Task 3 (Client: DID-Auth header builder)
Task 4 (Client: New DB tables)
    ↓
Task 5 (Client: UCAN store)               ← needs Task 4
Task 6 (Client: identity-auth Ed25519)     ← needs Task 3
    ↓
Task 7 (Client: HTTP auth migration)       ← needs Task 3, 5, 6
Task 8 (Client: WebSocket composable)      ← needs Task 1, 3
    ↓
Task 9 (Client: Sync engine migration)     ← needs Task 7, 8
    ↓
Task 10 (Client: Spaces Store refactor)    ← needs Task 5, 7
Task 11 (Client: Invite UI + spam)         ← needs Task 4, 10
Task 12 (Client: Change validator)         ← needs Task 5
```

Tasks 1-2 (server) and Tasks 3-4 (client) can run in parallel.

---

## Task 1: Server WebSocket endpoint with DID-Auth

**Project:** haex-sync-server (`/home/haex/Projekte/haex-sync-server`)

**Files:**
- Create: `src/routes/ws.ts`
- Modify: `index.ts` (register WS route)

**What to build:**

A WebSocket endpoint at `/ws` using Hono's Bun WebSocket support. Auth via DID-signed query parameter.

**Step 1: Create WebSocket route**

```typescript
// src/routes/ws.ts
import { Hono } from 'hono'
import { createBunWebSocket } from 'hono/bun'
import { didToPublicKey } from '@haex-space/ucan'
import { db, identities, spaceMembers } from '../db'
import { eq } from 'drizzle-orm'

const { upgradeWebSocket, websocket } = createBunWebSocket()

// Connected clients: Map<did, Set<WSContext>>
const connections = new Map<string, Set<any>>()

// Space memberships cache: Map<did, Set<spaceId>>
const membershipCache = new Map<string, Set<string>>()

function base64urlDecode(str: string): Uint8Array {
  let base64 = str.replace(/-/g, '+').replace(/_/g, '/')
  while (base64.length % 4 !== 0) base64 += '='
  const binary = atob(base64)
  return Uint8Array.from(binary, (c) => c.charCodeAt(0))
}

async function verifyWsAuth(token: string): Promise<{ did: string } | null> {
  const dotIndex = token.indexOf('.')
  if (dotIndex === -1) return null

  const payloadB64 = token.substring(0, dotIndex)
  const signatureB64 = token.substring(dotIndex + 1)

  let payload: { did: string; action: string; timestamp: number; bodyHash: string }
  try {
    payload = JSON.parse(new TextDecoder().decode(base64urlDecode(payloadB64)))
  } catch { return null }

  if (!payload.did || payload.action !== 'ws-connect') return null
  if (Math.abs(Date.now() - payload.timestamp) > 30_000) return null

  try {
    const publicKeyBytes = didToPublicKey(payload.did)
    const publicKey = await crypto.subtle.importKey('raw', publicKeyBytes, { name: 'Ed25519' }, false, ['verify'])
    const valid = await crypto.subtle.verify('Ed25519', publicKey, base64urlDecode(signatureB64), new TextEncoder().encode(payloadB64))
    if (!valid) return null
  } catch { return null }

  // Verify identity exists
  const [identity] = await db.select({ did: identities.did })
    .from(identities).where(eq(identities.did, payload.did)).limit(1)
  if (!identity) return null

  return { did: payload.did }
}

async function loadMemberships(did: string): Promise<Set<string>> {
  const memberships = await db.select({ spaceId: spaceMembers.spaceId })
    .from(spaceMembers).where(eq(spaceMembers.did, did))
  return new Set(memberships.map(m => m.spaceId))
}

const wsApp = new Hono()

wsApp.get('/ws', upgradeWebSocket(async (c) => {
  const token = c.req.query('token')
  if (!token) {
    return { onOpen(_, ws) { ws.close(4001, 'Missing auth token'); } }
  }

  const auth = await verifyWsAuth(token)
  if (!auth) {
    return { onOpen(_, ws) { ws.close(4001, 'Invalid auth'); } }
  }

  const did = auth.did

  return {
    async onOpen(_, ws) {
      // Register connection
      if (!connections.has(did)) connections.set(did, new Set())
      connections.get(did)!.add(ws)

      // Load space memberships
      const spaces = await loadMemberships(did)
      membershipCache.set(did, spaces)
    },
    onClose(_, ws) {
      connections.get(did)?.delete(ws)
      if (connections.get(did)?.size === 0) {
        connections.delete(did)
        membershipCache.delete(did)
      }
    },
  }
}))

/**
 * Broadcast event to all connected members of a space.
 * Called internally after push, invite, MLS operations.
 */
export function broadcastToSpace(spaceId: string, event: object, excludeDid?: string) {
  for (const [did, spaces] of membershipCache.entries()) {
    if (did === excludeDid) continue
    if (!spaces.has(spaceId)) continue
    const sockets = connections.get(did)
    if (!sockets) continue
    const message = JSON.stringify(event)
    for (const ws of sockets) {
      try { ws.send(message) } catch { /* connection may be closing */ }
    }
  }
}

/**
 * Send event to a specific DID (e.g. invite notification).
 */
export function sendToDid(did: string, event: object) {
  const sockets = connections.get(did)
  if (!sockets) return
  const message = JSON.stringify(event)
  for (const ws of sockets) {
    try { ws.send(message) } catch {}
  }
}

/**
 * Update membership cache when members are added/removed.
 */
export function updateMembershipCache(did: string, spaceId: string, action: 'add' | 'remove') {
  const spaces = membershipCache.get(did)
  if (!spaces) return
  if (action === 'add') spaces.add(spaceId)
  else spaces.delete(spaceId)
}

export { wsApp, websocket }
```

**Step 2: Register in index.ts**

In `index.ts`, import and mount the WebSocket route + export the websocket handler for Bun:

```typescript
import { wsApp, websocket } from './src/routes/ws'

// Mount WebSocket route
app.route('/', wsApp)

// Update export for Bun to include websocket handler
export default {
  port,
  fetch: app.fetch,
  websocket,
}
```

**Step 3: Commit**

```bash
cd /home/haex/Projekte/haex-sync-server
git add src/routes/ws.ts index.ts
git commit -m "feat: add WebSocket endpoint with DID-Auth and space broadcasting"
```

---

## Task 2: Server push triggers WebSocket events

**Project:** haex-sync-server

**Files:**
- Modify: `src/routes/sync.ts` (after push → broadcast)
- Modify: `src/routes/mls.ts` (after invite/message/welcome → broadcast/send)

**What to build:**

After a successful sync push, broadcast `{ type: "sync", spaceId }` to all space members via WebSocket. After invite creation, send `{ type: "invite" }` to invitee. After MLS message, broadcast `{ type: "mls" }`.

**Step 1: Add broadcasts to sync.ts**

After the successful push transaction (around line 260 in current sync.ts), add:

```typescript
import { broadcastToSpace } from './ws'

// After successful push:
const callerDid = getCallerDid(c)
broadcastToSpace(spaceId, { type: 'sync', spaceId }, callerDid) // exclude sender
```

**Step 2: Add broadcasts to mls.ts**

After invite creation:
```typescript
import { broadcastToSpace, sendToDid } from './ws'

// After creating invite:
sendToDid(body.inviteeDid, { type: 'invite', spaceId, inviteId: invite.id })

// After MLS message:
broadcastToSpace(spaceId, { type: 'mls', spaceId }, callerDid)

// After invite accept:
broadcastToSpace(spaceId, { type: 'membership', spaceId })
```

**Step 3: Add broadcasts to spaces.ts**

After member removal or space deletion:
```typescript
import { broadcastToSpace, updateMembershipCache } from './ws'

// After member added (invite accept finalized):
updateMembershipCache(memberDid, spaceId, 'add')
broadcastToSpace(spaceId, { type: 'membership', spaceId })

// After member removed:
updateMembershipCache(memberDid, spaceId, 'remove')
broadcastToSpace(spaceId, { type: 'membership', spaceId })
```

**Step 4: Commit**

```bash
git add src/routes/sync.ts src/routes/mls.ts src/routes/spaces.ts
git commit -m "feat: broadcast WebSocket events after sync push, invites, and MLS operations"
```

---

## Task 3: Client DID-Auth header builder

**Project:** haex-vault (`/home/haex/Projekte/haex-vault`)

**Files:**
- Create: `src/utils/auth/didAuth.ts`

**What to build:**

A utility function that creates `Authorization: DID <payload>.<signature>` headers using the identity's Ed25519 private key.

```typescript
// src/utils/auth/didAuth.ts
import { importUserPrivateKeyAsync } from '@haex-space/vault-sdk'

function base64urlEncode(data: Uint8Array): string {
  return btoa(String.fromCharCode(...data))
    .replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}

/**
 * Create a DID-Auth Authorization header.
 * Format: DID <base64url(payload)>.<base64url(signature)>
 */
export async function createDidAuthHeader(
  privateKeyBase64: string,
  did: string,
  action: string,
  body: string = '',
): Promise<string> {
  const bodyHash = base64urlEncode(new Uint8Array(
    await crypto.subtle.digest('SHA-256', new TextEncoder().encode(body)),
  ))

  const payload = JSON.stringify({
    did,
    action,
    timestamp: Date.now(),
    bodyHash,
  })

  const payloadEncoded = base64urlEncode(new TextEncoder().encode(payload))
  const payloadBytes = new TextEncoder().encode(payloadEncoded)

  const privateKey = await importUserPrivateKeyAsync(privateKeyBase64)
  const signature = new Uint8Array(
    await crypto.subtle.sign('Ed25519', privateKey, payloadBytes),
  )

  return `DID ${payloadEncoded}.${base64urlEncode(signature)}`
}

/**
 * Create a DID-Auth query parameter for WebSocket connections.
 * Returns just the token part (without "DID " prefix).
 */
export async function createDidAuthToken(
  privateKeyBase64: string,
  did: string,
): Promise<string> {
  const header = await createDidAuthHeader(privateKeyBase64, did, 'ws-connect', '')
  return header.substring(4) // Remove "DID " prefix
}

/**
 * Fetch wrapper that adds DID-Auth header.
 */
export async function fetchWithDidAuth(
  url: string,
  privateKeyBase64: string,
  did: string,
  action: string,
  options: RequestInit = {},
): Promise<Response> {
  const body = typeof options.body === 'string' ? options.body : ''
  const authHeader = await createDidAuthHeader(privateKeyBase64, did, action, body)

  return fetch(url, {
    ...options,
    headers: {
      ...options.headers,
      'Authorization': authHeader,
      'Content-Type': 'application/json',
    },
  })
}
```

**Step 2: Commit**

```bash
git add src/utils/auth/didAuth.ts
git commit -m "feat: add DID-Auth header builder for Ed25519 signed requests"
```

---

## Task 4: Client new DB tables

**Project:** haex-vault

**Files:**
- Modify: `src/database/schemas/haex.ts` (add new tables)
- Modify: `src/database/tableNames.json` (register new tables)

**What to build:**

Add these gesynkten tables to the Drizzle schema:

```typescript
// In haex.ts, add after haex_space_keys:

export const haexUcanTokens = sqliteTable('haex_ucan_tokens', {
  id: text('id').primaryKey(),
  spaceId: text('space_id').notNull(),
  token: text('token').notNull(),
  capability: text('capability').notNull(), // 'space/admin' | 'space/invite' | etc.
  issuerDid: text('issuer_did').notNull(),
  audienceDid: text('audience_did').notNull(),
  issuedAt: integer('issued_at').notNull(),
  expiresAt: integer('expires_at').notNull(),
  // CRDT columns added automatically
})

export const haexPendingInvites = sqliteTable('haex_pending_invites', {
  id: text('id').primaryKey(),
  spaceId: text('space_id').notNull(),
  inviterDid: text('inviter_did').notNull(),
  inviterLabel: text('inviter_label'),
  spaceName: text('space_name'), // Unencrypted label from invite
  status: text('status').notNull().default('pending'), // 'pending' | 'accepted' | 'declined'
  includeHistory: integer('include_history', { mode: 'boolean' }).default(false),
  createdAt: text('created_at').notNull(),
  respondedAt: text('responded_at'),
  // CRDT columns added automatically
})

export const haexBlockedDids = sqliteTable('haex_blocked_dids', {
  id: text('id').primaryKey(),
  did: text('did').notNull().unique(),
  label: text('label'),
  blockedAt: text('blocked_at').notNull(),
  // CRDT columns added automatically
})

export const haexInvitePolicy = sqliteTable('haex_invite_policy', {
  id: text('id').primaryKey(),
  policy: text('policy').notNull().default('all'), // 'all' | 'contacts_only' | 'nobody'
  updatedAt: text('updated_at').notNull(),
  // CRDT columns added automatically
})
```

Also add corresponding entries to `tableNames.json` following the existing pattern.

**Commit:**

```bash
git add src/database/schemas/haex.ts src/database/tableNames.json
git commit -m "feat: add UCAN tokens, pending invites, blocked DIDs, and invite policy tables"
```

---

## Task 5: Client UCAN store

**Project:** haex-vault

**Files:**
- Create: `src/utils/auth/ucanStore.ts`

**What to build:**

UCAN management: create root UCANs for owned spaces, store delegated UCANs, retrieve for API calls.

```typescript
// src/utils/auth/ucanStore.ts
import {
  createUcan,
  createWebCryptoSigner,
  spaceResource,
  type Capability,
} from '@haex-space/ucan'
import { importUserPrivateKeyAsync } from '@haex-space/vault-sdk'

// In-memory cache: spaceId → encoded UCAN token
const ucanCache = new Map<string, string>()

/**
 * Create a self-signed root UCAN for a space where the user is admin.
 */
export async function createRootUcanAsync(
  did: string,
  privateKeyBase64: string,
  spaceId: string,
  expiresInSeconds: number = 30 * 24 * 3600, // 30 days
): Promise<string> {
  const privateKey = await importUserPrivateKeyAsync(privateKeyBase64)
  const sign = createWebCryptoSigner(privateKey)

  const token = await createUcan({
    issuer: did,
    audience: did,
    capabilities: { [spaceResource(spaceId)]: 'space/admin' },
    expiration: Math.floor(Date.now() / 1000) + expiresInSeconds,
  }, sign)

  ucanCache.set(spaceId, token)
  return token
}

/**
 * Create a delegated UCAN for inviting someone to a space.
 */
export async function delegateUcanAsync(
  issuerDid: string,
  privateKeyBase64: string,
  audienceDid: string,
  spaceId: string,
  capability: Capability,
  parentUcan: string,
  expiresInSeconds: number = 30 * 24 * 3600,
): Promise<string> {
  const privateKey = await importUserPrivateKeyAsync(privateKeyBase64)
  const sign = createWebCryptoSigner(privateKey)

  return createUcan({
    issuer: issuerDid,
    audience: audienceDid,
    capabilities: { [spaceResource(spaceId)]: capability },
    expiration: Math.floor(Date.now() / 1000) + expiresInSeconds,
    proofs: [parentUcan],
  }, sign)
}

/**
 * Get UCAN for a space from cache or DB. Returns null if not found or expired.
 */
export async function getUcanForSpaceAsync(
  spaceId: string,
  db: any, // Drizzle DB instance
  ucanTable: any, // haexUcanTokens table ref
): Promise<string | null> {
  // Check cache first
  const cached = ucanCache.get(spaceId)
  if (cached) {
    // TODO: check expiry
    return cached
  }

  // Look up from DB (implementation depends on how DB is accessed in haex-vault)
  // The caller should query haex_ucan_tokens where spaceId matches and expiresAt > now
  return null
}

/**
 * Build Authorization: UCAN header for a space.
 */
export async function createUcanAuthHeader(
  spaceId: string,
  db: any,
  ucanTable: any,
): Promise<string | null> {
  const token = await getUcanForSpaceAsync(spaceId, db, ucanTable)
  if (!token) return null
  return `UCAN ${token}`
}

/**
 * Fetch wrapper that adds UCAN auth header.
 */
export async function fetchWithUcanAuth(
  url: string,
  spaceId: string,
  ucanToken: string,
  options: RequestInit = {},
): Promise<Response> {
  return fetch(url, {
    ...options,
    headers: {
      ...options.headers,
      'Authorization': `UCAN ${ucanToken}`,
      'Content-Type': 'application/json',
    },
  })
}

export function clearUcanCache(spaceId?: string) {
  if (spaceId) ucanCache.delete(spaceId)
  else ucanCache.clear()
}
```

**Commit:**

```bash
git add src/utils/auth/ucanStore.ts
git commit -m "feat: add UCAN store for creating, caching, and retrieving capability tokens"
```

---

## Task 6: Client identity-auth flow to Ed25519

**Project:** haex-vault

**Files:**
- Modify: `src/stores/sync/engine/supabase.ts` (lines 160-198, 179-183: ECDSA → Ed25519)
- Modify: `src/composables/useCreateSyncConnection.ts` (registration flow)

**What to change:**

The DID challenge-response currently signs with ECDSA SHA-256. Change to Ed25519:

In `supabase.ts`, `didAuthenticateAsync()`:
```typescript
// OLD (lines 179-183):
const sig = await crypto.subtle.sign(
  { name: 'ECDSA', hash: 'SHA-256' },
  privateKey,
  new TextEncoder().encode(nonce),
)

// NEW:
const sig = await crypto.subtle.sign(
  'Ed25519',
  privateKey,
  new TextEncoder().encode(nonce),
)
```

Also update key import to use Ed25519 instead of ECDSA P-256.

The function still returns a Supabase JWT (for GoTrue/Realtime compatibility), but all API calls will use DID-Auth or UCAN instead.

**Commit:**

```bash
git add src/stores/sync/engine/supabase.ts src/composables/useCreateSyncConnection.ts
git commit -m "refactor: switch identity-auth challenge signing from ECDSA to Ed25519"
```

---

## Task 7: Client HTTP auth migration

**Project:** haex-vault

**Files:**
- Modify: `src/stores/spaces.ts` (replace fetchWithAuth/Bearer with DID-Auth and UCAN)
- Modify: `src/stores/sync/engine/vaultKey.ts` (vault key operations use DID-Auth)
- Modify: `src/stores/sync/engine/changes.ts` (push/pull auth headers)

**What to change:**

Replace all `Authorization: Bearer ${token}` with either:
- `Authorization: DID <payload>.<signature>` for identity-scoped operations
- `Authorization: UCAN <token>` for space-scoped operations

**In spaces.ts:**

Replace `fetchWithAuth()` (lines 101-112) and all calls to it:

```typescript
// For space creation (DID-Auth):
const res = await fetchWithDidAuth(
  `${serverUrl}/spaces`,
  identity.privateKey, identity.did, 'create-space',
  { method: 'POST', body: JSON.stringify(payload) },
)

// For space operations (UCAN):
const ucan = await getUcanForSpaceAsync(spaceId, ...)
const res = await fetchWithUcanAuth(
  `${serverUrl}/spaces/${spaceId}`,
  spaceId, ucan!,
  { method: 'GET' },
)
```

Remove `fetchWithAuth()` and `fetchWithSpaceToken()` helpers entirely.

**In vaultKey.ts:**

Replace Bearer token usage with DID-Auth for vault-key operations (personal vault, not space-scoped):

```typescript
// All vault-key endpoints now use DID-Auth
const res = await fetchWithDidAuth(
  `${serverUrl}/sync/vault-key`,
  identity.privateKey, identity.did, 'vault-key',
  { method: 'POST', body: JSON.stringify(payload) },
)
```

**In changes.ts (push/pull):**

For shared spaces: UCAN auth. For personal vaults: DID-Auth.

```typescript
// Shared space push:
const ucan = await getUcanForSpaceAsync(spaceId, ...)
headers['Authorization'] = `UCAN ${ucan}`

// Personal vault push:
headers['Authorization'] = await createDidAuthHeader(identity.privateKey, identity.did, 'sync-push', bodyString)
```

**Commit:**

```bash
git add src/stores/spaces.ts src/stores/sync/engine/vaultKey.ts src/stores/sync/engine/changes.ts
git commit -m "refactor: replace Bearer JWT auth with DID-Auth and UCAN across all HTTP calls"
```

---

## Task 8: Client WebSocket composable

**Project:** haex-vault

**Files:**
- Create: `src/composables/useRealtime.ts`

**What to build:**

WebSocket connection to sync server with auto-reconnect.

```typescript
// src/composables/useRealtime.ts
import { ref, onUnmounted } from 'vue'
import { createDidAuthToken } from '../utils/auth/didAuth'

export function useRealtime() {
  const connected = ref(false)
  let ws: WebSocket | null = null
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null
  let reconnectAttempts = 0
  const MAX_RECONNECT_DELAY = 30_000

  const handlers = new Map<string, Set<(event: any) => void>>()

  function on(type: string, handler: (event: any) => void) {
    if (!handlers.has(type)) handlers.set(type, new Set())
    handlers.get(type)!.add(handler)
    return () => handlers.get(type)?.delete(handler)
  }

  async function connect(serverUrl: string, privateKeyBase64: string, did: string) {
    if (ws?.readyState === WebSocket.OPEN) return

    const token = await createDidAuthToken(privateKeyBase64, did)
    const wsUrl = serverUrl.replace(/^http/, 'ws') + `/ws?token=${encodeURIComponent(token)}`

    ws = new WebSocket(wsUrl)

    ws.onopen = () => {
      connected.value = true
      reconnectAttempts = 0
    }

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data)
        const typeHandlers = handlers.get(data.type)
        if (typeHandlers) {
          for (const handler of typeHandlers) handler(data)
        }
      } catch { /* ignore malformed messages */ }
    }

    ws.onclose = () => {
      connected.value = false
      scheduleReconnect(serverUrl, privateKeyBase64, did)
    }

    ws.onerror = () => {
      ws?.close()
    }
  }

  function scheduleReconnect(serverUrl: string, privateKeyBase64: string, did: string) {
    if (reconnectTimer) return
    const delay = Math.min(1000 * 2 ** reconnectAttempts, MAX_RECONNECT_DELAY)
    reconnectAttempts++
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null
      connect(serverUrl, privateKeyBase64, did)
    }, delay)
  }

  function disconnect() {
    if (reconnectTimer) { clearTimeout(reconnectTimer); reconnectTimer = null }
    ws?.close()
    ws = null
    connected.value = false
  }

  onUnmounted(disconnect)

  return { connected, connect, disconnect, on }
}
```

**Commit:**

```bash
git add src/composables/useRealtime.ts
git commit -m "feat: add WebSocket realtime composable with auto-reconnect"
```

---

## Task 9: Client sync engine migration

**Project:** haex-vault

**Files:**
- Modify: `src/stores/sync/orchestrator/realtime.ts` (replace Supabase Realtime)
- Modify: `src/stores/sync/orchestrator/index.ts` (use WebSocket instead of Supabase channels)
- Modify: `src/stores/sync/engine/supabase.ts` (remove Supabase Realtime client usage)

**What to change:**

Replace the Supabase Realtime subscription (`postgres_changes` on `sync_changes` table) with the WebSocket composable.

**In orchestrator/realtime.ts:**

Replace the entire Supabase channel subscription with:

```typescript
import { useRealtime } from '../../../composables/useRealtime'

const realtime = useRealtime()

// On sync event from WebSocket:
realtime.on('sync', async (event) => {
  const { spaceId } = event
  // Find backend for this space and trigger pull
  await pullFromBackendAsync(backendId, spaceId)
})

realtime.on('membership', async (event) => {
  // Refresh space memberships
})

realtime.on('invite', async (event) => {
  // Store pending invite
})

realtime.on('mls', async (event) => {
  // Fetch MLS messages
})
```

**In orchestrator/index.ts:**

Replace `initSupabaseRealtimeAsync()` with WebSocket connect:

```typescript
// In startSyncAsync():
await realtime.connect(serverUrl, identity.privateKey, identity.did)

// In stopSyncAsync():
realtime.disconnect()
```

**In engine/supabase.ts:**

Remove Realtime-related configuration. Keep only what's needed for GoTrue (email OTP verification). Remove:
- `supabaseClientRef` usage for Realtime
- `registerAuthStateListener` for token refresh (DID-Auth is stateless, no refresh needed)
- Realtime timeout/heartbeat config

Keep:
- `didAuthenticateAsync()` — still needed for initial login (returns Supabase JWT for GoTrue)
- GoTrue OTP functions

**Commit:**

```bash
git add src/stores/sync/orchestrator/ src/stores/sync/engine/supabase.ts
git commit -m "refactor: replace Supabase Realtime with WebSocket composable in sync engine"
```

---

## Task 10: Client Spaces Store refactor

**Project:** haex-vault

**Files:**
- Modify: `src/stores/spaces.ts` (major refactor)

**What to change:**

This is the largest single task. The spaces store needs to:

1. **Remove key grant logic** — No more `generateSpaceKey()`, `encryptWithPublicKeyAsync()` for key grants, `getSpaceKeysAsync()`, `persistSpaceKeyAsync()`
2. **Create space with DID-Auth** — No key grant in creation payload
3. **Invite with UCAN** — Create delegated UCAN, send via API
4. **Accept invite with DID-Auth** — Upload KeyPackages
5. **Store membership in local DB** — `haex_space_members` via CRDT sync
6. **Store UCANs in local DB** — `haex_ucan_tokens`

**Key function replacements:**

| Old | New |
|-----|-----|
| `fetchWithAuth(url, opts)` | `fetchWithDidAuth()` or `fetchWithUcanAuth()` |
| `createSpaceAsync()` with keyGrant | `createSpaceAsync()` with DID-Auth, no keyGrant |
| `inviteMemberAsync()` with keyGrant | `inviteMemberAsync()` with UCAN delegation |
| `getSpaceKeysAsync()` | Removed (MLS handles encryption in Phase 5) |
| `joinSpaceFromInviteAsync()` | Accept invite, receive UCAN |
| `listSpacesAsync()` with decryption | Read from local `haex_spaces` (synced) |

**Remove entirely:**
- `spaceKeyCache` Map
- `getSpaceKey()`, `getSpaceKeysAsync()`, `persistSpaceKeyAsync()`, `deleteSpaceKeysAsync()`
- `encryptSpaceName()` / `decryptSpaceName()` via space key (names will be encrypted with MLS in Phase 5)
- All keyGrant-related code in invite flow

**Commit:**

```bash
git add src/stores/spaces.ts
git commit -m "refactor: spaces store to UCAN auth, remove key grants"
```

---

## Task 11: Client Invite UI + spam protection

**Project:** haex-vault

**Files:**
- Create: `src/components/haex/spaces/invites.vue` (invite list view)
- Modify: `src/components/haex/spaces/index.vue` (add invites tab/section)
- Create: `src/composables/useInvitePolicy.ts` (spam protection logic)

**What to build:**

1. **Invite list view** — Shows pending invites with Accept/Decline buttons
2. **Invite policy composable** — Checks incoming invites against policy and block list
3. **Integration** — WebSocket `invite` events trigger policy check, then show in UI or auto-decline

**useInvitePolicy.ts:**
```typescript
export async function shouldShowInvite(inviterDid: string): Promise<boolean> {
  // 1. Check blocked_dids → if blocked, return false
  // 2. Check invite_policy:
  //    - 'all' → return true
  //    - 'contacts_only' → check haexContacts for inviterDid's publicKey
  //    - 'nobody' → return false
}
```

**Commit:**

```bash
git add src/components/haex/spaces/invites.vue src/composables/useInvitePolicy.ts
git commit -m "feat: add invite view with spam protection via policy and block list"
```

---

## Task 12: Client change validator

**Project:** haex-vault

**Files:**
- Create: `src/composables/useChangeValidator.ts`

**What to build:**

Vault-side validation of incoming sync_changes (Ebene 2). In Phase 4: UCAN capability checks. MLS checks come in Phase 5.

```typescript
// src/composables/useChangeValidator.ts
import { validateUcan, createWebCryptoVerifier, spaceResource } from '@haex-space/ucan'

const verify = createWebCryptoVerifier()

export async function validateIncomingChange(change: {
  spaceId: string
  signedBy?: string
  signature?: string
}): Promise<{ valid: boolean; error?: string }> {
  // Personal vault: always valid (own data)
  if (!change.signedBy) return { valid: true }

  // Shared space: verify UCAN capability
  // 1. Look up UCAN for signedBy DID in local haex_ucan_tokens
  // 2. Validate UCAN chain + capability (space/write)
  // 3. Check record ownership rules

  // For Phase 4: basic capability check
  // Full MLS membership check comes in Phase 5
  return { valid: true } // Placeholder until full implementation
}
```

**Commit:**

```bash
git add src/composables/useChangeValidator.ts
git commit -m "feat: add vault-side change validator for UCAN capability checks"
```

---

## Implementation Order Summary

| Order | Task | Project | Effort |
|-------|------|---------|--------|
| 1 | WebSocket endpoint | sync-server | Medium |
| 2 | WS event triggers | sync-server | Small |
| 3 | DID-Auth header builder | haex-vault | Small |
| 4 | New DB tables | haex-vault | Small |
| 5 | UCAN store | haex-vault | Medium |
| 6 | Identity-auth Ed25519 | haex-vault | Small |
| 7 | HTTP auth migration | haex-vault | Large |
| 8 | WebSocket composable | haex-vault | Medium |
| 9 | Sync engine migration | haex-vault | Large |
| 10 | Spaces Store refactor | haex-vault | Large |
| 11 | Invite UI + spam | haex-vault | Medium |
| 12 | Change validator | haex-vault | Small |
