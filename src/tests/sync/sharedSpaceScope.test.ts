import { describe, it, expect } from 'vitest'
import {
  SHARED_SPACE_BUILTIN_TABLES,
  isBuiltinSharedSpaceTable,
  getBuiltinSharedSpacePolicy,
  rowPksMatchSpace,
} from '~/stores/sync/sharedSpaceScope'

/**
 * Regression coverage for the shared-space scope policy.
 *
 * The whitelist + scope rules are the contract that closes the cloud-sync
 * data-leak: a member of two spaces must never push rows from Space B over
 * Space A's backend. Any change to the table list or the rowPks rules
 * should land here first.
 */
describe('sharedSpaceScope', () => {
  describe('SHARED_SPACE_BUILTIN_TABLES', () => {
    it('lists every CRDT table that legitimately travels over a shared-space stream', () => {
      // Snapshot the whitelist so removing or silently relaxing an entry
      // forces an explicit test update + reviewer attention.
      expect(Object.keys(SHARED_SPACE_BUILTIN_TABLES).sort()).toEqual([
        'haex_device_mls_enrollments',
        'haex_mls_sync_keys',
        'haex_peer_shares',
        'haex_pending_invites',
        'haex_shared_space_sync',
        'haex_space_devices',
        'haex_space_members',
        'haex_spaces',
        'haex_sync_rules',
        'haex_ucan_tokens',
      ])
    })

    it('uses `id` as the spaceId source for `haex_spaces` (the row IS the space)', () => {
      expect(SHARED_SPACE_BUILTIN_TABLES.haex_spaces).toEqual({ kind: 'self' })
    })

    it('uses `space_id` column for every other built-in table', () => {
      for (const [table, policy] of Object.entries(SHARED_SPACE_BUILTIN_TABLES)) {
        if (table === 'haex_spaces') continue
        expect(policy).toEqual({ kind: 'spaceIdColumn', column: 'space_id' })
      }
    })
  })

  describe('isBuiltinSharedSpaceTable', () => {
    it('accepts every whitelisted table', () => {
      for (const table of Object.keys(SHARED_SPACE_BUILTIN_TABLES)) {
        expect(isBuiltinSharedSpaceTable(table)).toBe(true)
      }
    })

    it('rejects vault-private tables that must never cross a shared-space stream', () => {
      // These all live in the same DB but contain personal vault data —
      // never permissible on a shared backend regardless of who pushes.
      const vaultPrivate = [
        'haex_identities',
        'haex_identity_claims',
        'haex_vault_settings',
        'haex_workspaces',
        'haex_desktop_items',
        'haex_sync_backends',
        'haex_extensions',
        'haex_extension_migrations',
        'haex_devices',
      ]
      for (const table of vaultPrivate) {
        expect(isBuiltinSharedSpaceTable(table)).toBe(false)
      }
    })

    it('rejects unknown / extension table names (those go through the registry path)', () => {
      expect(isBuiltinSharedSpaceTable('ext_passwords_items')).toBe(false)
      expect(isBuiltinSharedSpaceTable('totally_unknown_table')).toBe(false)
      expect(isBuiltinSharedSpaceTable('')).toBe(false)
    })
  })

  describe('getBuiltinSharedSpacePolicy', () => {
    it('returns the policy for whitelisted tables', () => {
      expect(getBuiltinSharedSpacePolicy('haex_spaces')).toEqual({ kind: 'self' })
      expect(getBuiltinSharedSpacePolicy('haex_space_members')).toEqual({
        kind: 'spaceIdColumn',
        column: 'space_id',
      })
    })

    it('returns null for tables outside the whitelist', () => {
      expect(getBuiltinSharedSpacePolicy('haex_identities')).toBeNull()
      expect(getBuiltinSharedSpacePolicy('not_a_real_table')).toBeNull()
    })
  })

  describe('rowPksMatchSpace', () => {
    const spaceA = 'space-aaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa'
    const spaceB = 'space-bbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb'

    it('accepts a haex_spaces row whose primary key matches the expected space', () => {
      expect(rowPksMatchSpace('haex_spaces', JSON.stringify({ id: spaceA }), spaceA)).toBe(true)
    })

    it('rejects a haex_spaces row whose primary key is a different space (the leak case)', () => {
      // This is the exact assertion that would have caught the original
      // leak had the receive path used it: a row claiming to be Space B
      // arriving over a Space A pull.
      expect(rowPksMatchSpace('haex_spaces', JSON.stringify({ id: spaceB }), spaceA)).toBe(false)
    })

    it('rejects a haex_spaces row with malformed rowPks JSON', () => {
      expect(rowPksMatchSpace('haex_spaces', 'not-json', spaceA)).toBe(false)
      expect(rowPksMatchSpace('haex_spaces', '', spaceA)).toBe(false)
    })

    it('rejects a haex_spaces row with the wrong primary-key column', () => {
      expect(rowPksMatchSpace('haex_spaces', JSON.stringify({ space_id: spaceA }), spaceA)).toBe(false)
    })

    it('cannot decide from rowPks alone for spaceIdColumn tables (returns true so the push-side filter remains the gate)', () => {
      // Built-in tables other than haex_spaces carry their spaceId in a
      // non-PK column, so rowPks ({"id": "..."}) doesn't reveal it. The
      // policy is "trust the push-side filter for this case" — encoded
      // here so a future maintainer doesn't accidentally tighten this
      // and break legitimate flows.
      expect(rowPksMatchSpace('haex_space_members', JSON.stringify({ id: 'mem-1' }), spaceA)).toBe(true)
      expect(rowPksMatchSpace('haex_peer_shares', JSON.stringify({ id: 'share-1' }), spaceA)).toBe(true)
    })

    it('rejects unknown / non-whitelisted tables', () => {
      expect(rowPksMatchSpace('haex_identities', JSON.stringify({ id: 'whatever' }), spaceA)).toBe(false)
      expect(rowPksMatchSpace('totally_unknown', JSON.stringify({ id: spaceA }), spaceA)).toBe(false)
    })
  })
})
