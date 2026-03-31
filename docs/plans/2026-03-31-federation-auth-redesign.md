# Federation Auth Redesign — Zero-Trust Relay Model

## Problem

The current federation relay design loses the user's identity. When a request flows through `Client → Relay → Home`, the relay replaces the user's auth with its own FEDERATION auth. The home server sees only "Relay B asks for data" — not *which user* initiated the request.

A malicious relay can:
- Make requests on behalf of arbitrary users
- Escalate capabilities (reader acts as admin)
- Redirect requests to different spaces/servers

## Requirements

1. **User signs locally**: Every federated request carries the user's cryptographic signature. Created on the client, never modified by the relay.
2. **Request binding**: The signature binds to the specific request (user + relay + home server + space + action + content). No part can be reused for a different request.
3. **Relay validates**: The relay verifies the request is addressed to itself before forwarding.
4. **Home validates**: The home server verifies the user's identity, membership, and that the request targets itself.
5. **No manipulation**: The relay cannot modify the user's authorization without invalidating it.
6. **Expiry**: Each request has an explicit `expiresAt` to minimize replay windows.

## Design

### Two Auth Flows (Completely Separate)

**Direct (Client → own Home Server):**
- Standard DID-Auth: `Authorization: DID <payload>.<signature>`
- Payload: `{ did, action, timestamp, bodyHash }`
- No relay fields. No UCAN needed. Server knows the user.
- **Unchanged from current implementation.**

**Federated (Client → Relay → foreign Home Server):**
- Client builds a **Federated DID-Auth** token with extended fields
- Relay embeds it in the FEDERATION header and forwards to home server
- Home server verifies both FEDERATION auth (relay identity) and embedded user auth

### Federated DID-Auth Payload

```json
{
  "did": "did:key:z6Mk...",
  "action": "sync-pull",
  "timestamp": 1711900000000,
  "expiresAt": 1711900010000,
  "requestHash": "<SHA-256 hex>",
  "spaceId": "eec50fb2-d2a1-42e2-8c5a-b231d316231f",
  "serverDid": "did:web:home.example.com",
  "relayDid": "did:web:relay.example.com"
}
```

| Field | Purpose |
|-------|---------|
| `did` | User's DID (did:key). Signature proves identity. |
| `action` | What the user wants to do (e.g., `sync-pull`, `sync-push`) |
| `timestamp` | When the request was created |
| `expiresAt` | Hard expiry. Home server rejects if `now > expiresAt`. Client controls the window (recommended: 10s). |
| `requestHash` | `SHA-256(body + '?' + sortedQueryString)` — binds signature to exact request content. For GET: `SHA-256('?' + sortedQueryString)`. For POST: `SHA-256(body + '?' + sortedQueryString)`. |
| `spaceId` | Which space this request targets |
| `serverDid` | DID of the home server (target). Home server verifies this matches itself. |
| `relayDid` | DID of the relay server (intermediary). Relay verifies this matches itself. |

**Signature:** `Ed25519(userPrivateKey, base64url(JSON.stringify(payload)))`

**Header format:** `DID <base64url(payload)>.<base64url(signature)>`

### FEDERATION Payload (Relay → Home)

The relay wraps the user's auth in its FEDERATION header:

```json
{
  "did": "did:web:relay.example.com",
  "action": "federation-proxy-post",
  "timestamp": 1711900001000,
  "bodyHash": "<SHA-256 of forwarded body>",
  "ucan": "<server/relay UCAN>",
  "userAuthorization": "DID <user-payload>.<user-signature>"
}
```

The `userAuthorization` field contains the **complete, unmodified** DID-Auth header value from the user. The relay cannot modify it — any modification invalidates the user's Ed25519 signature.

### Validation Chain

```
Client                     Relay Server                Home Server
  │                            │                            │
  │ signs request with:        │                            │
  │  - did (self)              │                            │
  │  - relayDid (relay)        │                            │
  │  - serverDid (home)        │                            │
  │  - spaceId                 │                            │
  │  - requestHash             │                            │
  │  - expiresAt               │                            │
  │                            │                            │
  ├─── DID Auth ──────────────►│                            │
  │                            │ 1. Verify signature        │
  │                            │ 2. relayDid === my DID?    │
  │                            │ 3. User known to me?       │
  │                            │ 4. expiresAt > now?        │
  │                            │                            │
  │                            ├─── FEDERATION + userAuth ─►│
  │                            │                            │ 1. Verify FEDERATION sig
  │                            │                            │ 2. Verify relay UCAN chain
  │                            │                            │ 3. Extract userAuthorization
  │                            │                            │ 4. Verify user signature
  │                            │                            │ 5. serverDid === my DID?
  │                            │                            │ 6. expiresAt > now?
  │                            │                            │ 7. requestHash matches?
  │                            │                            │ 8. spaceId member check
  │                            │                            │ 9. Member role sufficient?
  │                            │                            │    - read: any member
  │                            │                            │    - write: member/owner/admin
  │                            │                            │    - admin: admin only
  │                            │                            │ 10. Quota check (writes bill
  │                            │                            │     to space owner's account)
  │                            │                            │
  │                            │◄──── Response ─────────────│
  │◄──── Response ─────────────│                            │
```

### Attack Scenarios Prevented

| Attack | Prevention |
|--------|-----------|
| Relay forges user request | User's Ed25519 signature — relay doesn't have the private key |
| Relay redirects to different server | `serverDid` in signed payload — home server rejects if mismatch |
| Relay redirects to different space | `spaceId` in signed payload — home server rejects if mismatch |
| Replay attack | `expiresAt` (10s window) + `timestamp` |
| Replay to different endpoint | `requestHash` includes query params — different endpoint = different hash |
| Replay with modified body | `requestHash` includes body — modified body = different hash |
| Evil user sends request via wrong relay | `relayDid` in signed payload — relay rejects if mismatch |
| Non-member accesses space | Home server checks `did` membership in `spaceId` |
| Reader escalates to admin | Home server checks member role against action (read/write/admin) |
| Reader pushes data via relay | Home server verifies role >= write for push operations |
| Writer creates invite tokens | Home server verifies role >= admin/invite for invite operations |
| Write exceeds quota | Home server bills writes to space owner's account, rejects if over quota |
| Self-signed UCAN chain | `findRootIssuer()` + membership check in `requireCapability()` |

### requestHash Computation

```typescript
function computeRequestHash(body: string, queryString: string): string {
  // Normalize: sort query parameters for deterministic hashing
  const params = new URLSearchParams(queryString)
  const sorted = new URLSearchParams([...params.entries()].sort())
  const input = body + '?' + sorted.toString()
  const hash = SHA256(input)
  return hexEncode(hash)
}

// POST with body and query: SHA-256("{"spaceId":"...","changes":[...]}?limit=100&spaceId=...")
// GET with query only:      SHA-256("?afterUpdatedAt=...&limit=100&spaceId=...")
// POST with body, no query: SHA-256("{"spaceId":"..."}?")
```

### What Changes

**Client (haex-vault):**
- `createDidAuthHeader()` gets new required params for federated requests: `spaceId`, `serverDid`, `relayDid`
- `requestHash` replaces `bodyHash` (includes query params)
- New `createFederatedDidAuthHeader()` function (or extend existing with federation params)

**Relay Server (haex-sync-server as relay):**
- `federationRelay()` extracts user Auth header, validates `relayDid`, forwards in FEDERATION payload
- `federatedProxyAsync()` embeds `userAuthorization` in FEDERATION payload

**Home Server (haex-sync-server as home):**
- `federationAuth.ts` middleware extracts `userAuthorization` from FEDERATION payload
- Verifies user signature, `serverDid`, `spaceId`, `expiresAt`, `requestHash`
- Resolves user DID → space member record (role: admin/owner/member/reader)
- Sets user context (DID, role) so route handlers work as if it's a direct request
- Route handlers enforce role-based access:
  - `sync/pull`: any member (read)
  - `sync/push`: member/owner/admin (write) + quota check against space owner
  - `spaces/:id/invites`: admin/invite capability
  - `spaces/:id/members`: admin for add/remove
- Quota: all writes via federation are billed to the space **owner's** account (same as current `resolveSpaceOwnerUserId()`)

**@haex-space/ucan (no changes):**
- UCAN validation already works correctly with `findRootIssuer()` + membership check

**E2E Tests:**
- `federation-helpers.ts` updated with new auth format
- All attack scenarios tested

### What Does NOT Change

- Direct DID-Auth (client → own server): unchanged
- UCAN token format: unchanged
- Database schema: unchanged
- MLS/Invite flows: unchanged (they use the relay, which now properly forwards user auth)
- `server/relay` UCAN: still used for FEDERATION auth (proves relay is authorized)
