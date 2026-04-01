# Phase 7.5 — Federation Client Integration

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Enable cross-server space invites so users on different sync servers can share spaces, with transparent federation relay so the client only talks to its own server after setup.

**Architecture:** Two-phase join: (1) invitee claims invite directly on home server (no identity registration needed), (2) invitee delegates `server/relay` UCAN to their relay server, which establishes federation with the home server. After setup, all operations route through the relay transparently. MLS routes get the same federation relay pattern already proven in `sync.ts`.

**Tech Stack:** TypeScript (Hono server + Vue client), `@haex-space/ucan` for UCAN delegation, Ed25519 DID-Auth, MLS (OpenMLS via Tauri/Rust)

**Repos:** `haex-sync-server` (server), `haex-vault` (client)

---

## Overview

### Current State
- Server-side federation (Phase 7.1–7.4) is complete: sync push/pull relay works
- MLS routes have NO federation relay
- Invite claim requires full identity registration (email + OTP) on the target server
- Client has no cross-server invite flow
- No mechanism for client to trigger federation setup

### Target State
- Cross-server invites work without identity registration on home server
- MLS routes transparently relay through federation
- Client detects cross-server invites and handles them
- Client delegates `server/relay` UCAN and triggers federation setup
- After setup, client only talks to its own (relay) server

### Flow Diagram

```
CLAIM PHASE (direct to home server):
  Invitee ──DID-Auth──▶ Home Server: POST /spaces/:id/invite-tokens/:token/claim
  Home Server: creates federated identity (publicKey from DID), stores KeyPackages
  Admin ──UCAN──▶ Home Server: finalize invite (MLS add_member + welcome)
  Invitee ──DID-Auth──▶ Home Server: fetch welcome, process MLS

FEDERATION SETUP PHASE:
  Invitee: creates server/relay UCAN (audience = relay server DID)
  Invitee ──▶ Relay Server: POST /federation/setup { spaceId, homeServerUrl, relayUcan }
  Relay Server ──FEDERATION──▶ Home Server: POST /federation/establish
  Invitee: updates space serverUrl → relay server, creates sync backend

STEADY STATE:
  Invitee ──UCAN──▶ Relay Server ──FEDERATION──▶ Home Server (transparent)
```

---

## Task 1: Server — Federated Identity for Cross-Server Invite Claims

**Goal:** Allow invite token claims from DIDs that don't have a full identity registration on the home server, by deriving publicKey from the DID.

**Files:**
- Modify: `haex-sync-server/src/routes/mls.ts` (claim endpoint, ~line 622)
- Modify: `haex-sync-server/src/routes/spaces.ts` (add member on claim)
- Create: `haex-sync-server/src/utils/didIdentity.ts` (DID → SPKI publicKey conversion)

### Step 1: Create DID-to-SPKI helper

Create `haex-sync-server/src/utils/didIdentity.ts`:

```typescript
import { didToPublicKey } from '@haex-space/ucan'

/**
 * Convert raw Ed25519 public key bytes (32 bytes) to SPKI-encoded base64 string.
 * SPKI wraps the raw key in an ASN.1 DER structure.
 *
 * Format: 30 2a 30 05 06 03 2b 65 70 03 21 00 <32 bytes>
 */
const ED25519_SPKI_PREFIX = new Uint8Array([
  0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
])

export function rawKeyToSpkiBase64(rawKey: Uint8Array): string {
  const spki = new Uint8Array(ED25519_SPKI_PREFIX.length + rawKey.length)
  spki.set(ED25519_SPKI_PREFIX)
  spki.set(rawKey, ED25519_SPKI_PREFIX.length)
  return btoa(String.fromCharCode(...spki))
}

/**
 * Resolve a DID to an SPKI-encoded base64 public key string.
 * Works for did:key method by extracting the Ed25519 key and wrapping it in SPKI.
 */
export function didToSpkiPublicKey(did: string): string {
  const rawBytes = didToPublicKey(did)
  return rawKeyToSpkiBase64(rawBytes)
}
```

### Step 2: Modify invite token claim to support unregistered DIDs

In `haex-sync-server/src/routes/mls.ts`, modify the claim endpoint (~line 622):

```typescript
// BEFORE (requires full identity):
const identity = await resolveDidIdentity(didAuth.did)
if (!identity) return c.json({ error: 'Identity not found' }, 404)

// AFTER (fallback to DID-derived identity):
import { didToSpkiPublicKey } from '../utils/didIdentity'

let identityPublicKey: string
let identityDid: string

const identity = await resolveDidIdentity(didAuth.did)
if (identity) {
  identityPublicKey = identity.publicKey
  identityDid = identity.did
} else {
  // Cross-server claim: derive publicKey from DID (no registration needed)
  identityPublicKey = didToSpkiPublicKey(didAuth.did)
  identityDid = didAuth.did
}
```

Then replace all `identity.publicKey` with `identityPublicKey` and `identity.did` with `identityDid` in the claim handler.

### Step 3: Auto-create space member on token claim

After the invite is created in the transaction, also add the claimer as a space member (if not already):

```typescript
// Inside the transaction, after inserting spaceInvites:
const label = body.label ?? identityDid.slice(0, 20)

await tx.insert(spaceMembers).values({
  spaceId,
  publicKey: identityPublicKey,
  did: identityDid,
  label,
  role: token.capability === 'space/admin' ? 'admin' : token.capability === 'space/read' ? 'reader' : 'member',
  invitedBy: token.createdByDid,
}).onConflictDoNothing()
```

Update the claim schema to accept an optional `label`:

```typescript
const claimTokenSchema = z.object({
  keyPackages: z.array(z.string()).min(1).max(20),
  label: z.string().max(200).optional(),
})
```

### Step 4: Test

```bash
cd /home/haex/Projekte/haex-sync-server
# Test that claim works without identity registration
# 1. Create a space + invite token with a registered identity
# 2. Claim the token with a DID that has NO identity on the server
# 3. Verify: space member created, KeyPackages stored, invite accepted
```

### Step 5: Commit

```bash
cd /home/haex/Projekte/haex-sync-server
git add src/utils/didIdentity.ts src/routes/mls.ts src/routes/spaces.ts
git commit -m "feat: allow cross-server invite claims without identity registration"
```

---

## Task 2: Server — Generic Federation MLS Proxy

**Goal:** Create a reusable proxy function for forwarding requests to a home server with FEDERATION auth, and add federation relay to all MLS routes.

**Files:**
- Modify: `haex-sync-server/src/services/federationClient.ts` (add generic proxy)
- Modify: `haex-sync-server/src/routes/mls.ts` (add relay early-returns)

### Step 1: Add generic federation proxy to federationClient.ts

```typescript
/**
 * Generic federation proxy — forwards an arbitrary request to the home server.
 * Used for MLS and space operations that need relay.
 *
 * Preserves the original request method, path, query, and body.
 * Replaces auth with FEDERATION auth header.
 */
export async function federatedProxyAsync(
  link: FederationLink,
  method: string,
  path: string,
  body?: string,
  query?: string,
): Promise<{ ok: boolean; status: number; data: unknown }> {
  const url = `${link.homeServerUrl}${path}${query ? `?${query}` : ''}`
  const action = `federation-proxy-${method.toLowerCase()}`

  const authHeader = await buildFederationAuthHeader(action, body ?? '', link.ucanToken)

  const response = await fetch(url, {
    method,
    headers: {
      ...(body ? { 'Content-Type': 'application/json' } : {}),
      'Authorization': authHeader,
    },
    ...(body ? { body } : {}),
    signal: AbortSignal.timeout(30_000),
  })

  const data = await response.json()
  return { ok: response.ok, status: response.status, data }
}
```

### Step 2: Add MLS federation relay helper

Add a helper at the top of `mls.ts` to check federation and proxy:

```typescript
import { getFederationLinkForSpace, federatedProxyAsync } from '../services/federationClient'

/**
 * Check if a space is federated and proxy the request to the home server.
 * Returns the proxied response, or null if the space is not federated.
 */
async function federationRelay(c: any, spaceId: string): Promise<Response | null> {
  const link = await getFederationLinkForSpace(spaceId)
  if (!link) return null

  const method = c.req.method
  const path = c.req.path
  const query = new URL(c.req.url).search.slice(1) // remove leading ?
  const body = method !== 'GET' ? await c.req.text() : undefined

  const result = await federatedProxyAsync(link, method, path, body, query || undefined)
  return c.json(result.data, result.status as any)
}
```

### Step 3: Add relay early-returns to each MLS route

At the top of every MLS route handler (before auth checks), add:

```typescript
// Federation relay check
const relayResponse = await federationRelay(c, spaceId)
if (relayResponse) return relayResponse
```

Routes to modify (all in `mls.ts`):
1. `POST /:spaceId/invites` (create invite)
2. `GET /:spaceId/invites` (list invites)
3. `POST /:spaceId/invites/:inviteId/accept` (accept invite)
4. `PATCH /:spaceId/invites/:inviteId/ucan` (set UCAN on invite)
5. `POST /:spaceId/mls/key-packages` (upload key packages)
6. `GET /:spaceId/mls/key-packages/:did` (fetch key package)
7. `POST /:spaceId/mls/messages` (send message)
8. `GET /:spaceId/mls/messages` (fetch messages)
9. `POST /:spaceId/mls/welcome` (send welcome)
10. `GET /:spaceId/mls/welcome` (fetch welcomes)
11. `POST /:spaceId/invite-tokens` (create token)
12. `GET /:spaceId/invite-tokens` (list tokens)
13. `DELETE /:spaceId/invite-tokens/:tokenId` (revoke token)
14. `POST /:spaceId/invite-tokens/:tokenId/claim` (claim token)

**Important:** The relay check must happen BEFORE auth middleware processes. Since `authDispatcher` runs on `/*`, move the federation check into each handler after `spaceId` extraction but before `requireCapability`.

### Step 4: Update federation auth middleware to accept proxied MLS requests

The home server's `federationAuth.ts` needs to accept federation-proxied requests on `/spaces/:spaceId/mls/*` paths. Currently it only protects `/federation/*` routes.

Add federation route matching for space MLS paths in `index.ts` — or better: the home server MLS routes should also accept FEDERATION auth via `authDispatcher`.

Modify `authDispatcher.ts` to handle FEDERATION auth on all routes (not just `/federation/*`):

```typescript
// In authDispatcher, the FEDERATION scheme check already exists.
// Ensure it sets the context correctly so MLS routes can use it.
// The federation context includes the server DID and UCAN capabilities.
```

On the home server side, when receiving a FEDERATION-proxied MLS request:
- The federation context proves the relay is authorized for the space
- MLS routes should accept either UCAN/DID auth OR FEDERATION auth
- For FEDERATION auth, the caller DID is the relay server DID (not the end user)

Add a helper to extract caller context from either auth type:

```typescript
function getCallerDid(c: any): string | null {
  const ucan = c.get('ucan')
  if (ucan) return ucan.issuerDid
  const didAuth = c.get('didAuth')
  if (didAuth) return didAuth.did
  const federation = c.get('federation')
  if (federation) return federation.serverDid
  return null
}
```

### Step 5: Test

```bash
cd /home/haex/Projekte/haex-sync-server
# Test: create a federated space, then verify MLS operations are proxied
# 1. Set up federation between two server instances
# 2. Upload key packages via relay → verify they appear on home server
# 3. Send MLS message via relay → verify it appears on home server
```

### Step 6: Commit

```bash
cd /home/haex/Projekte/haex-sync-server
git add src/services/federationClient.ts src/routes/mls.ts src/middleware/authDispatcher.ts
git commit -m "feat: add MLS federation relay for cross-server space operations"
```

---

## Task 3: Server — Space Routes Federation Relay

**Goal:** Add federation relay to space read operations (list members, space details) so the relay server can serve space info from the home server.

**Files:**
- Modify: `haex-sync-server/src/routes/spaces.ts`

### Step 1: Add relay to GET space routes

Same pattern as MLS — add `federationRelay()` early-returns to:
1. `GET /:spaceId` (space details + members)
2. `POST /:spaceId/members` (invite member)
3. `DELETE /:spaceId/members/:memberDid` (remove member)

```typescript
import { getFederationLinkForSpace, federatedProxyAsync } from '../services/federationClient'

// Reuse the same federationRelay helper from mls.ts
// (or extract to a shared utility)
```

**Note:** `GET /` (list my spaces) should NOT be relayed — it lists spaces on THIS server. Only space-specific routes need relay.

### Step 2: Commit

```bash
cd /home/haex/Projekte/haex-sync-server
git add src/routes/spaces.ts
git commit -m "feat: add federation relay to space routes"
```

---

## Task 4: Server — Federation Setup Endpoint

**Goal:** New endpoint for clients to request their relay server to establish federation with a home server for a specific space.

**Files:**
- Modify: `haex-sync-server/src/routes/federation.ts` (add setup endpoint)
- Modify: `haex-sync-server/src/services/federationClient.ts` (add establish call)

### Step 1: Add `POST /federation/setup` endpoint

This endpoint is called by the CLIENT on their OWN (relay) server. It uses DID-Auth or UCAN auth (not FEDERATION auth — the client is a user, not a server).

```typescript
/**
 * POST /federation/setup
 * Called by a client to request this server to establish federation
 * with a home server for a specific space.
 *
 * Auth: DID-Auth or UCAN (the client is a space member)
 * Body: { spaceId, homeServerUrl, relayUcan }
 *
 * The relayUcan is a server/relay UCAN created by the client,
 * delegating relay capability to this server's DID.
 */
const setupSchema = z.object({
  spaceId: z.string().uuid(),
  homeServerUrl: z.string().url(),
  relayUcan: z.string().min(1), // server/relay UCAN for this server
})
```

This endpoint should NOT be behind `federationAuthMiddleware` — it uses regular user auth via `authDispatcher`.

Add it as a new route in the main Hono app, or as part of a user-facing federation routes file.

### Step 2: Implement the setup logic

```typescript
async function handleFederationSetup(c: any) {
  const body = await parseBody(c, setupSchema)
  if (body instanceof Response) return body

  if (!isFederationEnabled()) {
    return c.json({ error: 'Federation not enabled on this server' }, 503)
  }

  const serverIdentity = getServerIdentity()
  if (!serverIdentity) {
    return c.json({ error: 'Server identity not configured' }, 503)
  }

  // 1. Resolve home server identity
  const didDocUrl = `${body.homeServerUrl}/.well-known/did.json`
  const didDocResponse = await fetch(didDocUrl, { signal: AbortSignal.timeout(10_000) })
  if (!didDocResponse.ok) {
    return c.json({ error: 'Failed to resolve home server DID document' }, 502)
  }
  const didDoc = await didDocResponse.json()
  const homeServerDid = didDoc.id
  const homeServerPublicKeyHex = didDoc.verificationMethod?.[0]?.publicKeyMultibase // adapt to actual format

  // 2. Upsert home server in federation_servers
  const [server] = await db
    .insert(federationServers)
    .values({
      did: homeServerDid,
      url: body.homeServerUrl,
      publicKey: homeServerPublicKeyHex,
      name: didDoc.name ?? body.homeServerUrl,
    })
    .onConflictDoUpdate({
      target: federationServers.did,
      set: { url: body.homeServerUrl, updatedAt: new Date() },
    })
    .returning({ id: federationServers.id })

  // 3. Create local federation link (relay → home)
  await db
    .insert(federationLinks)
    .values({
      spaceId: body.spaceId,
      serverId: server!.id,
      ucanToken: body.relayUcan,
      ucanExpiresAt: extractUcanExpiry(body.relayUcan),
      role: 'relay',
    })
    .onConflictDoUpdate({
      target: [federationLinks.spaceId, federationLinks.serverId],
      set: {
        ucanToken: body.relayUcan,
        ucanExpiresAt: extractUcanExpiry(body.relayUcan),
        updatedAt: new Date(),
      },
    })

  // 4. Establish federation with home server
  const establishBody = JSON.stringify({
    spaceId: body.spaceId,
    serverUrl: `${new URL(c.req.url).origin}`,
    serverName: serverIdentity.did,
  })

  const authHeader = await buildFederationAuthHeader(
    'federation-establish',
    establishBody,
    body.relayUcan,
  )

  const establishResponse = await fetch(`${body.homeServerUrl}/federation/establish`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Authorization': authHeader,
    },
    body: establishBody,
    signal: AbortSignal.timeout(15_000),
  })

  if (!establishResponse.ok) {
    const error = await establishResponse.json().catch(() => ({}))
    return c.json({
      error: 'Failed to establish federation with home server',
      details: error,
    }, 502)
  }

  // 5. Connect to home server's federation WebSocket for real-time notifications
  // (done asynchronously, non-blocking)
  connectToHomeFederationWs(body.homeServerUrl, body.relayUcan).catch(err => {
    console.warn('[Federation] Failed to connect WS to home server:', err)
  })

  return c.json({ success: true, homeServerDid })
}
```

### Step 3: Add GET /federation/server-did endpoint

The client needs to know the relay server's DID to create the `server/relay` UCAN:

```typescript
/**
 * GET /federation/server-did
 * Returns this server's federation DID.
 * Public endpoint (no auth required).
 */
federation.get('/federation/server-did', (c) => {
  const identity = getServerIdentity()
  if (!identity) {
    return c.json({ error: 'Federation not configured' }, 404)
  }
  return c.json({ did: identity.did })
})
```

### Step 4: Commit

```bash
cd /home/haex/Projekte/haex-sync-server
git add src/routes/federation.ts src/services/federationClient.ts
git commit -m "feat: add federation setup endpoint for client-initiated relay"
```

---

## Task 5: Server — Relay WebSocket Client

**Goal:** When a relay server has federated spaces, it should connect to the home server's federation WebSocket to receive real-time notifications and re-broadcast them to local clients.

**Files:**
- Create: `haex-sync-server/src/services/federationWsClient.ts`
- Modify: `haex-sync-server/src/routes/federation.ts` (call on setup)

### Step 1: Create federation WebSocket client

```typescript
/**
 * Federation WebSocket Client
 *
 * Connects to home servers' /federation/ws endpoints.
 * Receives real-time events (sync, membership, mls) and re-broadcasts
 * them to local clients via broadcastToSpace().
 */

import { broadcastToSpace } from '../routes/ws'
import { getServerIdentity, signWithServerKeyAsync } from './serverIdentity'

const activeConnections = new Map<string, WebSocket>() // homeServerUrl → WebSocket

export async function connectToHomeFederationWs(
  homeServerUrl: string,
  ucanToken: string,
): Promise<void> {
  // Don't double-connect
  if (activeConnections.has(homeServerUrl)) return

  const token = await buildFederationWsToken(ucanToken)
  const wsUrl = `${homeServerUrl.replace(/^http/, 'ws')}/federation/ws?token=${encodeURIComponent(token)}`

  const ws = new WebSocket(wsUrl)

  ws.onopen = () => {
    activeConnections.set(homeServerUrl, ws)
    console.log(`[Federation WS] Connected to ${homeServerUrl}`)
  }

  ws.onmessage = (event) => {
    try {
      const data = JSON.parse(event.data as string)
      if (data.spaceId) {
        // Re-broadcast to local clients
        broadcastToSpace(data.spaceId, data)
      }
    } catch {
      // Ignore malformed messages
    }
  }

  ws.onclose = () => {
    activeConnections.delete(homeServerUrl)
    // Reconnect after delay
    setTimeout(() => {
      connectToHomeFederationWs(homeServerUrl, ucanToken).catch(() => {})
    }, 5000)
  }
}

async function buildFederationWsToken(ucanToken: string): Promise<string> {
  const identity = getServerIdentity()
  if (!identity) throw new Error('Server identity not initialized')

  const payload = {
    did: identity.did,
    action: 'federation-ws-connect',
    timestamp: Date.now(),
    bodyHash: btoa(String.fromCharCode(...new Uint8Array(
      await crypto.subtle.digest('SHA-256', new TextEncoder().encode(''))
    ))),
    ucan: ucanToken,
  }

  const payloadJson = JSON.stringify(payload)
  const payloadEncoded = base64urlEncode(new TextEncoder().encode(payloadJson))
  const signature = await signWithServerKeyAsync(new TextEncoder().encode(payloadEncoded))
  const signatureEncoded = base64urlEncode(signature)

  return `${payloadEncoded}.${signatureEncoded}`
}

function base64urlEncode(bytes: Uint8Array): string {
  return btoa(String.fromCharCode(...bytes))
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '')
}

/** Disconnect all federation WebSocket connections */
export function disconnectAllFederationWs(): void {
  for (const [url, ws] of activeConnections) {
    ws.close()
    activeConnections.delete(url)
  }
}
```

### Step 2: Initialize on server startup

On startup, query `federation_links` for all active relay links and connect to their home servers.

In `index.ts` or startup script:

```typescript
import { connectToHomeFederationWs } from './services/federationWsClient'

// After server starts, connect to all federated home servers
async function initFederationWsConnections() {
  const links = await db
    .select({
      url: federationServers.url,
      ucanToken: federationLinks.ucanToken,
    })
    .from(federationLinks)
    .innerJoin(federationServers, eq(federationLinks.serverId, federationServers.id))
    .where(eq(federationLinks.role, 'relay'))

  for (const link of links) {
    connectToHomeFederationWs(link.url, link.ucanToken).catch(err => {
      console.warn(`[Federation] Failed to connect to ${link.url}:`, err)
    })
  }
}
```

### Step 3: Commit

```bash
cd /home/haex/Projekte/haex-sync-server
git add src/services/federationWsClient.ts index.ts
git commit -m "feat: federation WebSocket client for relay-to-home notifications"
```

---

## Task 6: Client — Cross-Server Invite Detection & Claim

**Goal:** When a user opens an invite link for a space on a different server, detect this and handle the cross-server claim flow.

**Files:**
- Modify: `haex-vault/src/stores/spaces.ts` (new `claimCrossServerInviteAsync`)
- Modify: `haex-vault/src/components/haex/system/settings/spaces.vue` (UI integration)

### Step 1: Add helper to detect cross-server invites

In `spaces.ts`:

```typescript
/**
 * Check if an invite is cross-server (home server differs from user's sync server).
 * Returns the user's relay server URL if cross-server, null if same-server.
 */
const detectCrossServerInvite = (homeServerUrl: string): string | null => {
  const backendsStore = useSyncBackendsStore()
  const userServerUrl = backendsStore.backends[0]?.serverUrl
  if (!userServerUrl) return null

  // Normalize URLs for comparison
  const normalize = (url: string) => url.replace(/\/+$/, '').toLowerCase()
  if (normalize(homeServerUrl) === normalize(userServerUrl)) return null

  return userServerUrl
}
```

### Step 2: Modify claimInviteTokenAsync for cross-server support

```typescript
const claimInviteTokenAsync = async (
  serverUrl: string, // home server URL (from invite link)
  spaceId: string,
  tokenId: string,
  identityId: string,
): Promise<{ capability: string }> => {
  const identity = await resolveIdentityAsync(identityId)
  const relayServerUrl = detectCrossServerInvite(serverUrl)

  // Step 1: Claim invite on home server (works for both same-server and cross-server)
  const packages: number[][] = await invoke('mls_get_key_packages', { count: 10 })
  const keyPackagesBase64 = packages.map((p) => btoa(String.fromCharCode(...new Uint8Array(p))))

  const { fetchWithDidAuth: fetchDid } = await import('@/utils/auth/didAuth')
  const body = JSON.stringify({
    keyPackages: keyPackagesBase64,
    label: identity.label,
  })
  const response = await fetchDid(
    `${serverUrl}/spaces/${spaceId}/invite-tokens/${tokenId}/claim`,
    identity.privateKey,
    identity.did,
    'accept-invite',
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body,
    },
  )

  if (!response.ok) {
    const error = await response.json().catch(() => ({}))
    throw new Error(`Failed to claim invite: ${error.error || response.statusText}`)
  }

  const data = await response.json()

  if (relayServerUrl) {
    // Cross-server: set up federation, then persist space pointing to relay
    await setupFederationForSpaceAsync(relayServerUrl, serverUrl, spaceId, identityId)

    const space: SpaceWithType = {
      id: spaceId,
      name: '',
      type: 'shared',
      role: mapCapabilityToRole(data.capability),
      serverUrl: relayServerUrl, // Points to relay, not home server
      createdAt: new Date().toISOString(),
    }
    await persistSpaceAsync(space)
  } else {
    // Same-server: existing behavior
    const space: SpaceWithType = {
      id: spaceId,
      name: '',
      type: 'shared',
      role: mapCapabilityToRole(data.capability),
      serverUrl,
      createdAt: new Date().toISOString(),
    }
    await persistSpaceAsync(space)
  }

  log.info(`Claimed invite token for space ${spaceId} (capability: ${data.capability}, cross-server: ${!!relayServerUrl})`)
  return { capability: data.capability }
}

function mapCapabilityToRole(capability: string): SpaceRole {
  if (capability === 'space/admin') return SpaceRoles.ADMIN
  if (capability === 'space/read') return SpaceRoles.READER
  return SpaceRoles.MEMBER
}
```

### Step 3: Commit

```bash
cd /home/haex/Projekte/haex-vault
git add src/stores/spaces.ts
git commit -m "feat: cross-server invite detection and claim flow"
```

---

## Task 7: Client — Server/Relay UCAN Delegation & Federation Setup

**Goal:** After claiming a cross-server invite, create a `server/relay` UCAN for the relay server and trigger federation setup.

**Files:**
- Modify: `haex-vault/src/stores/spaces.ts` (new `setupFederationForSpaceAsync`)
- Modify: `haex-vault/src/utils/auth/ucanStore.ts` (support `server/relay` UCAN creation)

### Step 1: Add relay server DID resolution

In `spaces.ts`:

```typescript
/**
 * Fetch the relay server's federation DID.
 */
const getRelayServerDidAsync = async (relayServerUrl: string): Promise<string> => {
  const response = await fetch(`${relayServerUrl}/federation/server-did`)
  if (!response.ok) throw new Error('Relay server does not support federation')
  const data = await response.json()
  return data.did
}
```

### Step 2: Create server/relay UCAN

In `ucanStore.ts`, the existing `delegateUcanAsync` already supports arbitrary capabilities. The `@haex-space/ucan` library has `ServerCapability = 'server/relay'`. We need to use `serverResource(serverDid)` as the resource key.

```typescript
import { serverResource } from '@haex-space/ucan'

/**
 * Create a server/relay UCAN delegating relay capability to a server.
 */
export async function createServerRelayUcanAsync(
  issuerDid: string,
  issuerPrivateKeyBase64: string,
  serverDid: string,
  spaceId: string,
  parentUcan: string,
): Promise<string> {
  const { createUcan } = await import('@haex-space/ucan')

  const sign = await createEd25519SignFn(issuerPrivateKeyBase64)

  const token = await createUcan({
    issuer: issuerDid,
    audience: serverDid,
    capabilities: { [serverResource(serverDid)]: 'server/relay' },
    proofs: [parentUcan],
    expiration: Math.floor(Date.now() / 1000) + 30 * 24 * 60 * 60, // 30 days
  }, sign)

  return token
}
```

### Step 3: Implement setupFederationForSpaceAsync

In `spaces.ts`:

```typescript
/**
 * Set up federation for a space: delegate server/relay UCAN and tell relay server
 * to establish federation with the home server.
 */
const setupFederationForSpaceAsync = async (
  relayServerUrl: string,
  homeServerUrl: string,
  spaceId: string,
  identityId: string,
) => {
  const identity = await resolveIdentityAsync(identityId)

  // 1. Get relay server's DID
  const relayServerDid = await getRelayServerDidAsync(relayServerUrl)

  // 2. Get our UCAN for this space (needed as proof)
  const parentUcan = getUcanForSpaceAsync(spaceId)
  if (!parentUcan) {
    log.warn('No UCAN for space yet — federation setup deferred until UCAN received')
    return
  }

  // 3. Create server/relay UCAN delegated to relay server
  const { createServerRelayUcanAsync } = await import('@/utils/auth/ucanStore')
  const relayUcan = await createServerRelayUcanAsync(
    identity.did,
    identity.privateKey,
    relayServerDid,
    spaceId,
    parentUcan,
  )

  // 4. Tell relay server to establish federation
  const response = await fetchWithDidAuth(
    `${relayServerUrl}/federation/setup`,
    identity.privateKey,
    identity.did,
    'federation-setup',
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        spaceId,
        homeServerUrl,
        relayUcan,
      }),
    },
  )

  if (!response.ok) {
    const error = await response.json().catch(() => ({}))
    throw new Error(`Failed to set up federation: ${error.error || response.statusText}`)
  }

  log.info(`Federation established: relay ${relayServerUrl} → home ${homeServerUrl} for space ${spaceId}`)
}
```

### Step 4: Handle deferred federation setup

The UCAN may not be available immediately after claiming (admin must finalize first). Add a mechanism to retry federation setup when the UCAN becomes available:

```typescript
/**
 * Pending federation setups — spaces that need federation but don't have UCAN yet.
 * Checked when processing welcomes/invites.
 */
const pendingFederationSetups = ref<Array<{
  relayServerUrl: string
  homeServerUrl: string
  spaceId: string
  identityId: string
}>>([])

/**
 * Try to complete any pending federation setups.
 * Called after receiving a UCAN for a space.
 */
const completePendingFederationSetupsAsync = async () => {
  const remaining: typeof pendingFederationSetups.value = []

  for (const setup of pendingFederationSetups.value) {
    const ucan = getUcanForSpaceAsync(setup.spaceId)
    if (ucan) {
      try {
        await setupFederationForSpaceAsync(
          setup.relayServerUrl,
          setup.homeServerUrl,
          setup.spaceId,
          setup.identityId,
        )
      } catch (e) {
        log.warn(`Federation setup failed for ${setup.spaceId}, will retry: ${e}`)
        remaining.push(setup)
      }
    } else {
      remaining.push(setup)
    }
  }

  pendingFederationSetups.value = remaining
}
```

### Step 5: Commit

```bash
cd /home/haex/Projekte/haex-vault
git add src/stores/spaces.ts src/utils/auth/ucanStore.ts
git commit -m "feat: server/relay UCAN delegation and federation setup"
```

---

## Task 8: Client — Sync Backend Creation for Federated Spaces

**Goal:** After federation is established, ensure the sync backend points to the relay server so CRDT sync routes through federation.

**Files:**
- Modify: `haex-vault/src/stores/spaces.ts` (create backend after federation setup)
- Modify: `haex-vault/src/stores/sync/backends.ts` (if needed)

### Step 1: Create sync backend after federation setup

In `setupFederationForSpaceAsync`, after step 4 succeeds:

```typescript
// 5. Create sync backend pointing to relay server
const backendsStore = useSyncBackendsStore()
const existingBackend = backendsStore.backends.find(b => b.spaceId === spaceId)

if (!existingBackend) {
  await backendsStore.addBackendAsync({
    name: `Federation: ${spaceId.slice(0, 8)}`,
    serverUrl: relayServerUrl,
    spaceId,
    identityId: identity.publicKey,
    enabled: true,
    priority: 0,
  })
  log.info(`Created sync backend for federated space ${spaceId}`)
}
```

### Step 2: Commit

```bash
cd /home/haex/Projekte/haex-vault
git add src/stores/spaces.ts
git commit -m "feat: auto-create sync backend for federated spaces"
```

---

## Task 9: Client — Realtime WebSocket for Federated Spaces

**Goal:** Ensure the client's WebSocket connects to the relay server and receives federation-forwarded events.

**Files:**
- Modify: `haex-vault/src/composables/useRealtime.ts` (verify compatibility)

### Step 1: Verify WebSocket works with relay server

The current `useRealtime.ts` connects to `serverUrl/ws` with DID-Auth. Since the space's `serverUrl` now points to the relay server (after federation setup), the WebSocket will automatically connect to the relay server.

The relay server:
1. Authenticates the client via DID-Auth (existing flow)
2. Adds the client to the membership cache for the space
3. When the relay receives a federation WS event from the home server, it re-broadcasts to local clients via `broadcastToSpace()`

**This should work without client-side changes.** The key requirement is that the relay server has the client registered as a space member.

### Step 2: Verify space membership on relay server

When the invitee sets up federation, the relay server needs to know this client is a member of the space. The relay server's `spaceMembers` table needs an entry.

Option A: The relay server auto-creates a space member entry during federation setup.
Option B: The client explicitly registers as a member on the relay server.

Add to `POST /federation/setup` on the relay server:

```typescript
// After establishing federation, create local space + member entries
const callerDid = getCallerDid(c) // from DID-Auth
if (callerDid) {
  const identity = await resolveDidIdentity(callerDid)
  const publicKey = identity?.publicKey ?? didToSpkiPublicKey(callerDid)

  // Create space record locally
  await db.insert(spaces).values({
    id: body.spaceId,
    ownerId: callerDid,
    type: 'shared',
  }).onConflictDoNothing()

  // Create member record locally
  await db.insert(spaceMembers).values({
    spaceId: body.spaceId,
    publicKey,
    did: callerDid,
    label: 'Federation member',
    role: 'member',
  }).onConflictDoNothing()

  // Update WS membership cache
  updateMembershipCache(callerDid, body.spaceId, 'add')
}
```

### Step 3: Commit

```bash
cd /home/haex/Projekte/haex-sync-server
git add src/routes/federation.ts
git commit -m "feat: auto-register space members on relay during federation setup"
```

---

## Task 10: Integration Testing

**Goal:** Verify the full cross-server invite and federation flow works end-to-end.

### Test Scenario

**Setup:**
- Server A (home): `http://localhost:3001`
- Server B (relay): `http://localhost:3002`
- User Alice: registered on Server A, admin of Space X
- User Bob: registered on Server B, wants to join Space X

**Flow:**
1. Alice creates invite token on Server A for Space X
2. Bob claims token on Server A (DID-Auth, no registration on A)
3. Alice finalizes invite (MLS add_member, welcome)
4. Bob processes welcome from Server A
5. Bob creates server/relay UCAN for Server B
6. Bob calls `POST /federation/setup` on Server B
7. Server B establishes federation with Server A
8. Bob switches space serverUrl to Server B
9. Bob pushes changes → Server B relays to Server A
10. Alice pushes changes → Server A notifies Server B → Bob receives via WS

### Manual Test Script

Create `haex-sync-server/scripts/test-federation-flow.ts`:

```typescript
// Automated test script that:
// 1. Starts two server instances
// 2. Registers identities
// 3. Creates space + invite
// 4. Claims cross-server
// 5. Sets up federation
// 6. Verifies sync relay works
```

### Step 1: Commit

```bash
cd /home/haex/Projekte/haex-sync-server
git add scripts/test-federation-flow.ts
git commit -m "test: add federation integration test script"
```

---

## Implementation Order & Dependencies

```
Task 1 (federated identity)  ──────────┐
Task 2 (MLS relay)           ──────────┤
Task 3 (space routes relay)  ──────────┤── Server-side (can be parallelized)
Task 4 (setup endpoint)      ──────────┤
Task 5 (relay WS client)     ──────────┘
                                        │
Task 6 (client claim)        ─── depends on Task 1
Task 7 (UCAN + setup)        ─── depends on Task 4
Task 8 (sync backend)        ─── depends on Task 7
Task 9 (realtime)            ─── depends on Task 5
Task 10 (integration test)   ─── depends on all above
```

**Recommended execution:**
1. Tasks 1–5 in parallel (server-side, independent)
2. Tasks 6–9 sequentially (client-side, chained)
3. Task 10 last (integration test)
