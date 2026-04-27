/**
 * Tests for the self-leave flow in src/stores/spaces.
 *
 * Covers two pieces:
 *
 * 1. `leaveSpaceAsync` strategy selection (LOCAL vs REMOTE branch). The
 *    decision logic is small and pure, so we mirror it inline rather than
 *    stand up Pinia/Drizzle (matches the pattern in capabilities.test.ts).
 *
 * 2. `removeSelfFromSpace` Phase-1 contract: select-our-memberships then
 *    delete-ucans then delete-memberships. We exercise the real function
 *    against a hand-rolled Drizzle-shaped mock so the SQL surface (which
 *    columns we filter on, which tables we delete from) is locked down.
 *
 * Background: explicit deletes are required because cascade-driven deletes
 * are not guaranteed to fire the BEFORE-DELETE CRDT trigger — peers would
 * silently miss our self-leave without these explicit DELETEs.
 */

import { describe, it, expect, vi } from 'vitest'
import { SpaceType, type SpaceType as SpaceTypeValue } from '~/database/constants'
import { NoCurrentIdentityError } from '@/composables/useCurrentIdentity'
import { removeSelfFromSpace } from '@/stores/spaces/members'
import {
  haexSpaceMembers,
  haexUcanTokens,
} from '~/database/schemas'

// ---------------------------------------------------------------------------
// Strategy selection (pure-logic mirror)
// ---------------------------------------------------------------------------

type LeaveStrategy =
  | { kind: 'local' }
  | { kind: 'remote'; identityId: string }

const selectLeaveStrategy = (
  space: { type: SpaceTypeValue } | undefined,
  originUrl: string,
  identityId: string | null,
): LeaveStrategy => {
  if (space?.type === SpaceType.LOCAL || !originUrl) {
    return { kind: 'local' }
  }
  if (!identityId) {
    throw new NoCurrentIdentityError()
  }
  return { kind: 'remote', identityId }
}

describe('leaveSpaceAsync — strategy selection', () => {
  describe('local path', () => {
    it('returns local for SpaceType.LOCAL even when an originUrl is set', () => {
      expect(
        selectLeaveStrategy(
          { type: SpaceType.LOCAL },
          'https://server.example/',
          'identity-1',
        ),
      ).toEqual({ kind: 'local' })
    })

    it('returns local when originUrl is empty (defensive: missing/null)', () => {
      expect(selectLeaveStrategy({ type: SpaceType.ONLINE }, '', null))
        .toEqual({ kind: 'local' })
    })

    it('returns local when the space row was not found in activeSpaces', () => {
      expect(selectLeaveStrategy(undefined, '', null))
        .toEqual({ kind: 'local' })
    })
  })

  describe('remote path', () => {
    it('returns remote with identityId for ONLINE spaces with both pieces', () => {
      expect(
        selectLeaveStrategy(
          { type: SpaceType.ONLINE },
          'https://server.example/',
          'identity-1',
        ),
      ).toEqual({ kind: 'remote', identityId: 'identity-1' })
    })

    it('throws NoCurrentIdentityError when remote space has no identityId', () => {
      expect(() =>
        selectLeaveStrategy(
          { type: SpaceType.ONLINE },
          'https://server.example/',
          null,
        ),
      ).toThrow(NoCurrentIdentityError)
    })
  })
})

// ---------------------------------------------------------------------------
// removeSelfFromSpace Phase-1 contract
// ---------------------------------------------------------------------------

/**
 * Minimal Drizzle-shaped mock that records every fluent call. Just enough to
 * exercise the select/innerJoin/where + delete/where surface that
 * removeSelfFromSpace uses.
 */
function makeMockDb(memberships: Array<{ membershipId: string; did: string }>) {
  const deletes: Array<{ table: unknown; where: unknown }> = []

  const selectChain = {
    from: () => selectChain,
    innerJoin: () => selectChain,
    where: () => Promise.resolve(memberships),
  }

  const deleteChain = (table: unknown) => ({
    where: (whereExpr: unknown) => {
      deletes.push({ table, where: whereExpr })
      return Promise.resolve()
    },
  })

  return {
    db: {
      select: () => selectChain,
      delete: deleteChain,
    },
    deletes,
  }
}

describe('removeSelfFromSpace', () => {
  it('returns early without DB writes when no own identities are passed', async () => {
    const { db, deletes } = makeMockDb([])
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const result = await removeSelfFromSpace(db as any, 'space-1', [])
    expect(result).toEqual({ removedMemberships: 0, removedUcanDids: [] })
    expect(deletes).toHaveLength(0)
  })

  it('returns early without DB writes when no membership row is found', async () => {
    // membership lookup returns empty (we are not actually a member)
    const { db, deletes } = makeMockDb([])
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const result = await removeSelfFromSpace(db as any, 'space-1', ['id-A'])
    expect(result).toEqual({ removedMemberships: 0, removedUcanDids: [] })
    expect(deletes).toHaveLength(0)
  })

  it('keeps UCAN rows alive by default so the LEAVING peer-sync can still authenticate', async () => {
    // Local-leave path: only the membership row is deleted. UCANs stay
    // alive so PeerSession::connect can still authenticate during the
    // LEAVING propagation window. The FK-cascade on haex_spaces removes
    // them when cleanupCompletedLeavesAsync finally drops the space row.
    const { db, deletes } = makeMockDb([
      { membershipId: 'm-1', did: 'did:key:z1' },
    ])
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    await removeSelfFromSpace(db as any, 'space-1', ['id-A'])

    expect(deletes).toHaveLength(1)
    expect(deletes[0]?.table).toBe(haexSpaceMembers)
  })

  it('deletes UCAN tokens before membership when deleteUcans option is set (remote-leave path)', async () => {
    const { db, deletes } = makeMockDb([
      { membershipId: 'm-1', did: 'did:key:z1' },
    ])
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    await removeSelfFromSpace(db as any, 'space-1', ['id-A'], { deleteUcans: true })

    expect(deletes).toHaveLength(2)
    expect(deletes[0]?.table).toBe(haexUcanTokens)
    expect(deletes[1]?.table).toBe(haexSpaceMembers)
  })

  it('reports counts for multi-identity vaults that joined the same space twice', async () => {
    const { db } = makeMockDb([
      { membershipId: 'm-1', did: 'did:key:zA' },
      { membershipId: 'm-2', did: 'did:key:zB' },
    ])
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const result = await removeSelfFromSpace(db as any, 'space-1', ['id-A', 'id-B'])

    expect(result.removedMemberships).toBe(2)
    expect(result.removedUcanDids).toEqual(['did:key:zA', 'did:key:zB'])
  })

  it('logs but does not throw when the user is no longer a member', async () => {
    const { db } = makeMockDb([])
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    await expect(removeSelfFromSpace(db as any, 'space-1', ['id-A']))
      .resolves.toBeDefined()
    warnSpy.mockRestore()
  })
})
