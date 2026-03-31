# Federation Auth Redesign — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement zero-trust federation auth where every federated request carries a user-signed, non-manipulable authorization that binds to user + relay + home server + space + request content.

**Architecture:** The client creates a federated DID-Auth header with extended fields (`spaceId`, `serverDid`, `relayDid`, `requestHash`, `expiresAt`). The relay validates `relayDid`, then embeds the user auth 1:1 in the FEDERATION payload. The home server extracts and fully validates the user auth including membership and role checks.

**Tech Stack:** TypeScript (Hono server, Vue client), Ed25519 DID-Auth, `@haex-space/ucan`

**Repos:** `haex-sync-server`, `haex-vault`, `haex-e2e-tests`

**Design doc:** `docs/plans/2026-03-31-federation-auth-redesign.md`

---

## Task 1: Client — Federated DID-Auth Header

**Goal:** New function `createFederatedDidAuthHeader` that produces a DID-Auth token with all federation fields.

**Files:**
- Modify: `haex-vault/src/utils/auth/didAuth.ts`

### Implementation

Add new function after existing `createDidAuthHeader`:

```typescript
export interface FederatedAuthParams {
  spaceId: string
  serverDid: string
  relayDid: string
}

export async function createFederatedDidAuthHeader(
  privateKeyBase64: string,
  did: string,
  action: string,
  federation: FederatedAuthParams,
  body?: string,
  queryString?: string,
): Promise<string> {
  // requestHash = SHA-256(body + '?' + sortedQueryString)
  const params = new URLSearchParams(queryString ?? '')
  const sorted = new URLSearchParams([...params.entries()].sort())
  const hashInput = (body ?? '') + '?' + sorted.toString()
  const requestHash = hexEncode(
    new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(hashInput)))
  )

  const now = Date.now()
  const payload = JSON.stringify({
    did,
    action,
    timestamp: now,
    expiresAt: now + 10_000, // 10 second window
    requestHash,
    spaceId: federation.spaceId,
    serverDid: federation.serverDid,
    relayDid: federation.relayDid,
  })

  const payloadBytes = new TextEncoder().encode(payload)
  const base64urlPayload = base64urlEncode(payloadBytes)

  const privateKey = await importUserPrivateKeyAsync(privateKeyBase64)
  const signatureBuffer = await crypto.subtle.sign(
    'Ed25519',
    privateKey,
    new TextEncoder().encode(base64urlPayload),
  )

  return `DID ${base64urlPayload}.${base64urlEncode(new Uint8Array(signatureBuffer))}`
}

function hexEncode(bytes: Uint8Array): string {
  return Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('')
}
```

Also add `fetchWithFederatedDidAuth`:

```typescript
export async function fetchWithFederatedDidAuth(
  url: string,
  privateKeyBase64: string,
  did: string,
  action: string,
  federation: FederatedAuthParams,
  options?: RequestInit,
): Promise<Response> {
  const body = typeof options?.body === 'string' ? options.body : undefined
  const queryString = new URL(url).search.slice(1)
  const header = await createFederatedDidAuthHeader(
    privateKeyBase64, did, action, federation, body, queryString,
  )

  return fetch(url, {
    ...options,
    headers: {
      ...options?.headers,
      Authorization: header,
    },
  })
}
```

### Commit

```
feat: add federated DID-Auth with spaceId, serverDid, relayDid binding
```

---

## Task 2: Server — Validate Federated User Auth in Federation Middleware

**Goal:** When the home server receives a FEDERATION request with embedded `userAuthorization`, extract and fully validate the user's federated DID-Auth.

**Files:**
- Modify: `haex-sync-server/src/middleware/federationAuth.ts`
- Modify: `haex-sync-server/src/middleware/types.ts` (extend FederationContext)

### Implementation

**Extend `FederationContext`** in `types.ts`:

```typescript
export interface FederationContext {
  serverDid: string
  serverPublicKey: Uint8Array
  issuerDid: string
  ucanToken: string
  ucanCapabilities: Record<string, string>
  action: string
  // User auth from the federated request (if present)
  userDid?: string
  userRole?: string
  userSpaceId?: string
}
```

**Add user auth validation** in `federationAuth.ts` after the existing FEDERATION verification (after `c.set('federation', ...)`):

Extract `payload.userAuthorization` from the FEDERATION payload. If present:

1. Parse the DID-Auth token (same format: `DID <payload>.<signature>`)
2. Decode the federated payload: `{ did, action, timestamp, expiresAt, requestHash, spaceId, serverDid, relayDid }`
3. Verify `serverDid === this server's DID` (request is meant for us)
4. Verify `expiresAt > now` (not expired)
5. Verify Ed25519 signature (user really signed this)
6. Verify `requestHash` matches the actual request body + query
7. Look up user DID in `space_members` for `spaceId`
8. If not member → 403
9. Store `userDid`, `userRole`, `userSpaceId` in FederationContext

**New helper function** `verifyFederatedUserAuth`:

```typescript
async function verifyFederatedUserAuth(
  userAuthHeader: string,
  requestBody: string,
  requestQuery: string,
  thisServerDid: string,
): Promise<{ did: string; role: string; spaceId: string } | { error: string; status: number }>
```

This function:
- Strips `DID ` prefix
- Splits payload and signature
- Decodes and validates all fields
- Verifies Ed25519 signature using public key from `did:key`
- Computes `requestHash` from body + query and compares
- Queries `space_members` for membership + role
- Returns user context or error

**Call it** in `federationAuthMiddleware` after existing verification, before `await next()`:

```typescript
if (payload.userAuthorization) {
  const requestQuery = new URL(c.req.url).search.slice(1)
  const userResult = await verifyFederatedUserAuth(
    payload.userAuthorization,
    body, // already read earlier
    requestQuery,
    getServerIdentity()!.did,
  )
  if ('error' in userResult) {
    return c.json({ error: userResult.error }, userResult.status as any)
  }
  federationContext.userDid = userResult.did
  federationContext.userRole = userResult.role
  federationContext.userSpaceId = userResult.spaceId
}
```

### Commit

```
feat: validate federated user auth on home server with full request binding
```

---

## Task 3: Server — Relay Validates relayDid Before Forwarding

**Goal:** The relay server validates that the federated DID-Auth `relayDid` matches its own DID, and that the user is known, before forwarding.

**Files:**
- Modify: `haex-sync-server/src/routes/mls.ts` (`federationRelay` helper)
- Modify: `haex-sync-server/src/routes/spaces.ts` (`federationRelay` helper)
- Modify: `haex-sync-server/src/routes/sync.ts` (push/pull relay sections)

### Implementation

**In `mls.ts` and `spaces.ts`**, update the `federationRelay` function:

```typescript
import { getServerIdentity } from '../services/serverIdentity'

async function federationRelay(c: any, spaceId: string): Promise<Response | null> {
  const link = getFederationLinkForSpace(spaceId)
  if (!link) return null

  const userAuth = c.req.header('Authorization') ?? ''
  if (!userAuth) return c.json({ error: 'User authentication required for federated requests' }, 401)

  // Validate that this relay is the intended relay (relayDid check)
  if (userAuth.startsWith('DID ')) {
    const validated = validateRelayDid(userAuth)
    if (validated) return c.json({ error: validated }, 403)
  }

  const method = c.req.method
  const path = c.req.path
  const query = new URL(c.req.url).search.slice(1)
  const body = method !== 'GET' && method !== 'DELETE' ? await c.req.text() : undefined

  const result = await federatedProxyAsync(link, method, path, userAuth, body || undefined, query || undefined)
  return c.json(result.data, result.status as any)
}

/**
 * Validate that the federated DID-Auth relayDid matches this server.
 * Returns error message if invalid, undefined if valid.
 */
function validateRelayDid(authHeader: string): string | undefined {
  try {
    const token = authHeader.slice(4)
    const dotIndex = token.indexOf('.')
    if (dotIndex === -1) return 'Malformed auth token'

    const payloadEncoded = token.slice(0, dotIndex)
    let base64 = payloadEncoded.replace(/-/g, '+').replace(/_/g, '/')
    while (base64.length % 4 !== 0) base64 += '='
    const payload = JSON.parse(atob(base64))

    // Check relayDid if present (federated request)
    if (payload.relayDid) {
      const myDid = getServerIdentity()?.did
      if (myDid && payload.relayDid !== myDid) {
        return `Request not intended for this relay (expected ${myDid}, got ${payload.relayDid})`
      }
    }

    return undefined
  } catch {
    return 'Failed to parse auth payload'
  }
}
```

**In `sync.ts`**, same pattern for push and pull relay sections.

### Commit

```
feat: relay validates relayDid before forwarding federated requests
```

---

## Task 4: Server — Home Server Enforces Role-Based Access on Federated Requests

**Goal:** When a federated request arrives with user auth, the home server enforces that the user's role is sufficient for the operation. Writes bill to space owner.

**Files:**
- Modify: `haex-sync-server/src/routes/sync.ts` (push handler for federated requests)
- Modify: `haex-sync-server/src/routes/federation.ts` (push handler)

### Implementation

**In `sync.ts` federation push handler and `federation.ts` push handler**, after receiving a federated request that has user auth:

```typescript
// After extracting federation context:
const federation = c.get('federation')
if (federation?.userDid && federation?.userRole) {
  // Check role is sufficient for write
  if (federation.userRole === 'reader') {
    return c.json({ error: 'Insufficient permissions — reader cannot push data' }, 403)
  }
}
```

For `federation.ts` `/federation/push` handler, add the role check before inserting changes. The owner's userId is already resolved via `resolveSpaceOwnerUserId(spaceId)` for quota billing — this stays unchanged.

### Commit

```
feat: enforce role-based access control on federated write operations
```

---

## Task 5: Client — Use Federated DID-Auth in Space Store

**Goal:** When the client sends requests through a relay (federated spaces), use `createFederatedDidAuthHeader` instead of regular DID-Auth.

**Files:**
- Modify: `haex-vault/src/stores/spaces.ts` (`setupFederationForSpaceAsync` and related)
- Modify: `haex-vault/src/stores/sync/orchestrator/push.ts` (if push goes through relay)
- Modify: `haex-vault/src/stores/sync/orchestrator/pull.ts` (if pull goes through relay)

### Implementation

In `spaces.ts`, the `setupFederationForSpaceAsync` already calls `fetchWithDidAuth` for the federation setup request. Replace with `fetchWithFederatedDidAuth`:

```typescript
import { fetchWithFederatedDidAuth } from '@/utils/auth/didAuth'

// In setupFederationForSpaceAsync:
const response = await fetchWithFederatedDidAuth(
  `${relayServerUrl}/federation/setup`,
  identity.privateKey,
  identity.did,
  'federation-setup',
  { spaceId, serverDid: homeServerUrl, relayDid: relayServerDid },
  { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: ... },
)
```

For sync push/pull through relay: the sync engine uses `createDidAuthHeader` in `push.ts` and `pull.ts`. When the backend's space is federated (serverUrl points to relay), use `createFederatedDidAuthHeader` with the relay and home server DIDs.

The relay/home DIDs need to be stored alongside the sync backend config — add `relayDid` and `homeServerDid` fields to the in-memory backend state, populated during federation setup.

### Commit

```
feat: client uses federated DID-Auth for relay requests
```

---

## Task 6: E2E Tests — Update Federation Helpers and Tests

**Goal:** Update test helpers to use the new federated DID-Auth format and verify all security properties.

**Files:**
- Modify: `/tmp/haex-e2e-tests/tests/helpers/federation-helpers.ts`
- Modify: `/tmp/haex-e2e-tests/tests/federation/federation-security.spec.ts`
- Modify: `/tmp/haex-e2e-tests/tests/federation/relay-privilege-escalation.spec.ts`

### Implementation

**Update `federation-helpers.ts`:**

Add `createFederatedDidAuthHeader` (same implementation as client, using Node crypto):

```typescript
export async function createFederatedDidAuthHeader(
  privateKeyBase64: string,
  did: string,
  action: string,
  spaceId: string,
  serverDid: string,
  relayDid: string,
  body?: string,
  queryString?: string,
): Promise<string>
```

Update `pushChangesToServer`, `pullChangesFromServer`, `setupFederation`, `claimInviteToken` to use federated auth when going through relay.

**Update tests:**

- Existing attack tests should still pass (forged UCAN, wrong space, etc.)
- Add new tests:
  - Request with wrong `relayDid` is rejected by relay
  - Request with wrong `serverDid` is rejected by home server
  - Expired `expiresAt` is rejected
  - Modified body doesn't match `requestHash`
  - Reader via relay cannot push (role check)
  - Non-member via relay cannot pull (membership check)

### Commit

```
test: update federation E2E tests for zero-trust auth model
```

---

## Task 7: Commit, Push, Final Test Run

**Files:**
- Both repos: `haex-sync-server`, `haex-vault`

### Steps

1. Push `haex-sync-server` changes
2. Push `haex-vault` changes
3. Start both servers with federation keys
4. Run full E2E test suite
5. Verify all 26+ tests pass

### Commit

```
chore: push federation auth redesign
```

---

## Implementation Order & Dependencies

```
Task 1 (Client DID-Auth)         ──── independent
Task 2 (Home Server validation)  ──── independent
Task 3 (Relay validation)        ──── depends on Task 2 (shared types)
Task 4 (Role-based access)       ──── depends on Task 2
Task 5 (Client integration)      ──── depends on Task 1
Task 6 (E2E Tests)               ──── depends on all above
Task 7 (Final test run)          ──── depends on Task 6
```

**Recommended execution:** Tasks 1+2 in parallel, then 3+4, then 5, then 6+7.
