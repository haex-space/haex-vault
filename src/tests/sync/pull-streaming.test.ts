import { describe, it, expect } from 'vitest'
import { splitCompleteGroups, groupByHlcAscending } from '@/stores/sync/orchestrator/pull-helpers'
import type { ColumnChange } from '@/stores/sync/tableScanner'

// Build a `ColumnChange` carrying a given HLC. The other fields are irrelevant
// to the helpers under test (they partition / group purely on `hlcTimestamp`),
// so they get deterministic placeholders.
const change = (hlc: string, columnName = 'col'): ColumnChange => ({
  tableName: 'haex_passwords',
  rowPks: JSON.stringify({ id: `${hlc}-${columnName}` }),
  columnName,
  hlcTimestamp: hlc,
  deviceId: 'd1',
})

// HLC strings are `<time>/<node_hex>`; comparison is numeric on the time
// component, so A < B < C below are genuinely ascending.
const HLC_A = '100/aa'
const HLC_B = '200/aa'
const HLC_C = '300/aa'

const hlcs = (cs: ColumnChange[]) => cs.map((c) => c.hlcTimestamp)

describe('splitCompleteGroups', () => {
  it('returns empty for empty input regardless of hasMore', () => {
    expect(splitCompleteGroups([], false)).toEqual({ toApply: [], holdBack: [] })
    expect(splitCompleteGroups([], true)).toEqual({ toApply: [], holdBack: [] })
  })

  it('applies everything when hasMore=false (page is complete)', () => {
    const { toApply, holdBack } = splitCompleteGroups(
      [change(HLC_A, 'a'), change(HLC_B, 'b'), change(HLC_C, 'c')],
      false,
    )
    expect(hlcs(toApply)).toEqual([HLC_A, HLC_B, HLC_C])
    expect(holdBack).toHaveLength(0)
  })

  it('holds back the trailing (max-HLC) group when hasMore=true', () => {
    const { toApply, holdBack } = splitCompleteGroups(
      [change(HLC_A, 'a'), change(HLC_B, 'b'), change(HLC_C, 'c')],
      true,
    )
    expect(hlcs(toApply)).toEqual([HLC_A, HLC_B])
    expect(hlcs(holdBack)).toEqual([HLC_C])
  })

  it('holds back ALL changes when a single HLC equals the max and hasMore=true', () => {
    // A single transaction may not be fully delivered yet — never split it.
    const { toApply, holdBack } = splitCompleteGroups(
      [change(HLC_A, 'a'), change(HLC_A, 'b')],
      true,
    )
    expect(toApply).toHaveLength(0)
    expect(hlcs(holdBack)).toEqual([HLC_A, HLC_A])
  })

  it('keeps all changes at the trailing HLC together when hasMore=true', () => {
    // Multiple columns of the same source transaction (HLC_C) must all be
    // held back together — the transaction is never split across the page
    // boundary.
    const { toApply, holdBack } = splitCompleteGroups(
      [change(HLC_A, 'a'), change(HLC_C, 'c1'), change(HLC_C, 'c2')],
      true,
    )
    expect(hlcs(toApply)).toEqual([HLC_A])
    expect(hlcs(holdBack)).toEqual([HLC_C, HLC_C])
  })
})

describe('groupByHlcAscending', () => {
  it('returns empty array for empty input', () => {
    expect(groupByHlcAscending([])).toEqual([])
  })

  it('groups by hlcTimestamp and sorts groups ascending', () => {
    const groups = groupByHlcAscending([
      change(HLC_C, 'c1'),
      change(HLC_A, 'a1'),
      change(HLC_B, 'b1'),
      change(HLC_A, 'a2'),
      change(HLC_C, 'c2'),
    ])
    expect(groups.map((g) => g.hlc)).toEqual([HLC_A, HLC_B, HLC_C])
    expect(groups.map((g) => g.changes.length)).toEqual([2, 1, 2])
  })

  it('keeps changes for one HLC together (one source transaction = one group)', () => {
    const groups = groupByHlcAscending([
      change(HLC_A, 'a1'),
      change(HLC_A, 'a2'),
      change(HLC_A, 'a3'),
    ])
    expect(groups).toHaveLength(1)
    expect(groups[0]!.hlc).toBe(HLC_A)
    expect(groups[0]!.changes).toHaveLength(3)
  })

  it('compares HLCs numerically on the time component (not lexicographically)', () => {
    // Lexicographically "100" > "99" is FALSE, but numerically 100 > 99. The
    // helper must use numeric comparison via `hlcIsNewer`, so 99 sorts before
    // 100.
    const groups = groupByHlcAscending([change('100/aa', 'x'), change('99/aa', 'y')])
    expect(groups.map((g) => g.hlc)).toEqual(['99/aa', '100/aa'])
  })
})
