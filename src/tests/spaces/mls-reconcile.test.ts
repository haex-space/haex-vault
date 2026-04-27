/**
 * Tests for the leader-side MLS reconciliation diff contract.
 *
 * `reconcileMlsAfterMemberSyncAsync` in src/stores/spaces/reconcileMls.ts
 * compares a per-space DID snapshot with the current membership set and
 * triggers MLS rekey for each disappeared DID — but only if we are leader
 * and only on the second-and-later passes (the first call primes).
 *
 * The integration version requires Tauri invokes (mls_*, local_delivery_*)
 * which we do not stand up here. Instead we mirror the pure diff logic and
 * the first-pass guard so the contract is locked down.
 */

import { describe, it, expect } from 'vitest'

type DidSnapshot = Map<string, Set<string>>

interface DiffResult {
  primed: boolean
  removed: string[]
}

function diffMembersAndPrime(
  snapshots: DidSnapshot,
  spaceId: string,
  current: Set<string>,
): DiffResult {
  const isFirstPass = !snapshots.has(spaceId)
  const previous = snapshots.get(spaceId) ?? new Set<string>()
  snapshots.set(spaceId, current)

  if (isFirstPass) return { primed: true, removed: [] }

  const removed: string[] = []
  for (const did of previous) {
    if (!current.has(did)) removed.push(did)
  }
  return { primed: false, removed }
}

describe('reconcileMlsAfterMemberSyncAsync — diff contract', () => {
  it('first call primes the snapshot without reporting removals', () => {
    const snapshots: DidSnapshot = new Map()
    const result = diffMembersAndPrime(
      snapshots,
      'sp-1',
      new Set(['did:key:zA', 'did:key:zB']),
    )
    expect(result).toEqual({ primed: true, removed: [] })
    expect(snapshots.get('sp-1')).toEqual(new Set(['did:key:zA', 'did:key:zB']))
  })

  it('second call detects DIDs that disappeared since the snapshot', () => {
    const snapshots: DidSnapshot = new Map()
    diffMembersAndPrime(snapshots, 'sp-1', new Set(['did:key:zA', 'did:key:zB']))
    const result = diffMembersAndPrime(snapshots, 'sp-1', new Set(['did:key:zA']))
    expect(result.primed).toBe(false)
    expect(result.removed).toEqual(['did:key:zB'])
  })

  it('does not flag added members (only removals trigger MLS rekey)', () => {
    const snapshots: DidSnapshot = new Map()
    diffMembersAndPrime(snapshots, 'sp-1', new Set(['did:key:zA']))
    const result = diffMembersAndPrime(
      snapshots,
      'sp-1',
      new Set(['did:key:zA', 'did:key:zNew']),
    )
    expect(result.removed).toEqual([])
  })

  it('reports multiple simultaneous removals', () => {
    const snapshots: DidSnapshot = new Map()
    diffMembersAndPrime(
      snapshots,
      'sp-1',
      new Set(['did:key:zA', 'did:key:zB', 'did:key:zC']),
    )
    const result = diffMembersAndPrime(
      snapshots,
      'sp-1',
      new Set(['did:key:zA']),
    )
    expect(result.removed.sort()).toEqual(['did:key:zB', 'did:key:zC'])
  })

  it('keeps per-space snapshots independent', () => {
    const snapshots: DidSnapshot = new Map()
    diffMembersAndPrime(snapshots, 'sp-1', new Set(['did:key:zA']))
    const r2 = diffMembersAndPrime(snapshots, 'sp-2', new Set(['did:key:zX']))
    // sp-2 is its own first-pass even though sp-1 was already primed.
    expect(r2.primed).toBe(true)
  })

  it('updates snapshot even when no removals are detected (so future passes diff against latest)', () => {
    const snapshots: DidSnapshot = new Map()
    diffMembersAndPrime(snapshots, 'sp-1', new Set(['did:key:zA']))
    diffMembersAndPrime(snapshots, 'sp-1', new Set(['did:key:zA', 'did:key:zB']))
    const result = diffMembersAndPrime(snapshots, 'sp-1', new Set(['did:key:zA']))
    // The 2nd snapshot included zB; the 3rd diff detects zB as removed.
    expect(result.removed).toEqual(['did:key:zB'])
  })
})
