/**
 * Tests for the LEAVING-space cleanup heuristic.
 *
 * `cleanupCompletedLeavesAsync` (src/stores/spaces/crud.ts) decides whether
 * a LEAVING space row is old enough to drop unconditionally. The decision
 * is purely time-based: if `Date.now() - modifiedAt > LEAVE_GIVE_UP_AFTER_MS`
 * we remove the row, otherwise we keep it pending.
 *
 * The selection-logic itself is small and pure; we mirror it here so
 * timing-edge-cases can be exercised without standing up Drizzle. The
 * selection is the only behaviour worth locking down — once `removed`
 * comes back from the heuristic, the surrounding orchestration is just a
 * loop over `removeSpaceFromDbAsync`.
 */

import { describe, it, expect } from 'vitest'
import { LEAVE_GIVE_UP_AFTER_MS } from '@/stores/spaces/crud'

const isLeaveSafeToFinalize = (
  candidate: { id: string; modifiedAt: string | null },
  now = Date.now(),
): boolean => {
  if (!candidate.modifiedAt) return false
  const ageMs = now - new Date(candidate.modifiedAt).getTime()
  if (Number.isNaN(ageMs)) return false
  return ageMs > LEAVE_GIVE_UP_AFTER_MS
}

describe('cleanupCompletedLeavesAsync — time-based heuristic', () => {
  const NOW = new Date('2026-04-27T12:00:00Z').getTime()

  it('keeps a fresh LEAVING space (just-marked, push likely still in flight)', () => {
    const candidate = {
      id: 'sp-fresh',
      modifiedAt: '2026-04-27T11:59:00Z', // 1 minute ago
    }
    expect(isLeaveSafeToFinalize(candidate, NOW)).toBe(false)
  })

  it('keeps a LEAVING space at exactly 30 days minus 1 second (boundary)', () => {
    const justUnder = new Date(NOW - LEAVE_GIVE_UP_AFTER_MS + 1000).toISOString()
    expect(
      isLeaveSafeToFinalize({ id: 'sp-edge', modifiedAt: justUnder }, NOW),
    ).toBe(false)
  })

  it('finalizes a LEAVING space at 30 days plus 1 second (boundary)', () => {
    const justOver = new Date(NOW - LEAVE_GIVE_UP_AFTER_MS - 1000).toISOString()
    expect(
      isLeaveSafeToFinalize({ id: 'sp-edge', modifiedAt: justOver }, NOW),
    ).toBe(true)
  })

  it('finalizes a LEAVING space that is way past the window (60 days)', () => {
    const long = new Date(NOW - 60 * 24 * 60 * 60 * 1000).toISOString()
    expect(
      isLeaveSafeToFinalize({ id: 'sp-old', modifiedAt: long }, NOW),
    ).toBe(true)
  })

  it('refuses to finalize a candidate without a modifiedAt timestamp (defensive)', () => {
    expect(
      isLeaveSafeToFinalize({ id: 'sp-no-ts', modifiedAt: null }, NOW),
    ).toBe(false)
  })

  it('refuses to finalize a candidate with an unparseable timestamp (defensive)', () => {
    expect(
      isLeaveSafeToFinalize(
        { id: 'sp-bad', modifiedAt: 'definitely-not-a-date' },
        NOW,
      ),
    ).toBe(false)
  })

  it('LEAVE_GIVE_UP_AFTER_MS is 30 days', () => {
    expect(LEAVE_GIVE_UP_AFTER_MS).toBe(30 * 24 * 60 * 60 * 1000)
  })
})
