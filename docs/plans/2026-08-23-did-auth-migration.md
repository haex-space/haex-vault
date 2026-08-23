# DID-Auth shared PoP primitive migration

**Status:** Implemented locally against `@haex-space/ucan` 0.4.0.

## Goal

Make the DID-Auth client and sync-server middleware thin wrappers over the shared
`@haex-space/ucan` proof-of-possession primitives, while retaining the
`Authorization: DID <payload>.<signature>` scheme.

## Contract

- The common payload fields are `did`, `timestamp`, `exp`, `jti`, and
  `requestHash` (`PopPayload`). `exp = timestamp + 60_000`; `jti` is a fresh UUID
  and is replay-protected by the server's existing TTL cache.
- `action` is removed from DID-Auth: no DID-Auth route consumed it for
  authorization, and the signed request target is its meaningful context.
- `requestHash` is the base64url SHA-256 digest of
  `method + "\\n" + path + "\\n" + rawQuery + "\\n" + body` with no URL
  normalization. It replaces `bodyHash`.
- Client and server call `createSignedAuthHeader` / `verifySignedAuthHeader`;
  neither recreates the signed-header wire format nor computes a body hash.

### SDK compatibility finding

`@haex-space/ucan` 0.4.0 signs the frozen `PopPayload` shape directly.
`action` was removed because no DID-Auth route uses it for authorization; the
request line and body are the meaningful signed context.

## Threat model

The old body-only signature could be replayed against a different route with an
identical body (a URL-target swap). Binding method, path, and raw query to the
signature rejects that request while preserving DID and replay protections.

## Delivery order

1. Update `haex-vault` to consume `@haex-space/ucan` 0.4.0 and create the `DID`
   header through the shared primitive.
2. Update `haex-sync-server` to verify the same header through the shared
   primitive, including method/path/query/body inputs and its replay cache.
3. Add a Playwright attack spec in `haex-e2e-tests`: a valid DID signature for
   one protected URL must be rejected on a second URL with the same body.

The federation request-hash alignment is explicitly out of scope and follows in
its own change.

## Verification

- Focused client and server tests cover shared header creation/verification.
- The new E2E URL-target-swap attack spec is green alongside its valid baseline.
- Run formatting and the relevant test suites in all three repositories before
  opening PRs against `develop`.
