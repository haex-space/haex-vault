/**
 * Shared-space scope policy for cloud CRDT sync.
 *
 * Personal vault sync replicates the entire vault DB between a single user's
 * own devices, so any CRDT row may be pushed. Shared-space sync replicates
 * data between members of a single space, so the push/pull pipeline MUST
 * restrict itself to rows that belong to that space — otherwise a member who
 * happens to be in two spaces would push rows from Space B over the Space A
 * backend (encrypted with Space A's MLS key) and every Space A peer would
 * decrypt and ingest them. That is a data leak.
 *
 * This module defines:
 *   - the whitelist of CRDT tables that may travel over a shared-space sync
 *     stream and how to derive the spaceId from a row in each of them;
 *   - helpers used by both the outbound scanner (push) and the inbound
 *     validator (pull) so the rules stay in lockstep on both ends.
 *
 * Mirrors the Rust whitelist `SPACE_SCOPED_CRDT_TABLES` in
 * `src-tauri/src/crdt/scanner.rs` (which guards QUIC space-delivery).
 * The cloud sync needs its own copy because the QUIC scanner never sees
 * cloud-bound changes.
 */

/** How a row's owning spaceId is derived for a built-in table. */
export type SharedSpaceScopePolicy =
  | { kind: 'self' } // table IS the space (haex_spaces): row.id === spaceId
  | { kind: 'spaceIdColumn'; column: string } // row[column] === spaceId

/**
 * Tables whose rows may travel over a shared-space sync stream, with the
 * column the scanner uses to filter to the active space.
 *
 * Anything outside this map is either:
 *   - vault-private (identities, vault settings, workspaces, sync backends, …)
 *     and must never be shipped via shared-space sync, or
 *   - an extension table whose space scope lives in `haex_shared_space_sync`
 *     (handled separately by `scanTableForSpaceChangesAsync`).
 */
export const SHARED_SPACE_BUILTIN_TABLES: Record<string, SharedSpaceScopePolicy> = {
  haex_spaces: { kind: 'self' },
  haex_space_members: { kind: 'spaceIdColumn', column: 'space_id' },
  haex_space_devices: { kind: 'spaceIdColumn', column: 'space_id' },
  haex_peer_shares: { kind: 'spaceIdColumn', column: 'space_id' },
  haex_mls_sync_keys: { kind: 'spaceIdColumn', column: 'space_id' },
  haex_device_mls_enrollments: { kind: 'spaceIdColumn', column: 'space_id' },
  haex_ucan_tokens: { kind: 'spaceIdColumn', column: 'space_id' },
  haex_pending_invites: { kind: 'spaceIdColumn', column: 'space_id' },
  haex_sync_rules: { kind: 'spaceIdColumn', column: 'space_id' },
  haex_shared_space_sync: { kind: 'spaceIdColumn', column: 'space_id' },
}

export function isBuiltinSharedSpaceTable(tableName: string): boolean {
  return tableName in SHARED_SPACE_BUILTIN_TABLES
}

export function getBuiltinSharedSpacePolicy(
  tableName: string,
): SharedSpaceScopePolicy | null {
  return SHARED_SPACE_BUILTIN_TABLES[tableName] ?? null
}

/**
 * Receive-side check: returns `true` if a change with these PKs is allowed
 * over a shared-space sync stream targeting `spaceId`. Returns `false` for
 * tables we can rule out from rowPks alone (currently only `haex_spaces`).
 *
 * For `spaceIdColumn` tables we cannot decide from rowPks alone — the
 * spaceId is a non-PK column, so the actual filter has to live in the
 * push-side scanner. The whitelist alone (`isBuiltinSharedSpaceTable`)
 * is the receive-side guard for those.
 */
export function rowPksMatchSpace(
  tableName: string,
  rowPksJson: string,
  spaceId: string,
): boolean {
  const policy = SHARED_SPACE_BUILTIN_TABLES[tableName]
  if (!policy) return false
  if (policy.kind !== 'self') return true
  try {
    const pks = JSON.parse(rowPksJson) as Record<string, unknown>
    return pks.id === spaceId
  }
  catch {
    return false
  }
}
