import { describe, it, expect } from 'vitest'
import { chunkChangesByHlc } from '@/stores/sync/orchestrator/push'
import type { ColumnChange } from '@/stores/sync/tableScanner'

const makeChange = (hlc: string, columnName = 'col'): ColumnChange => ({
  tableName: 'test',
  rowPks: JSON.stringify({ id: `${hlc}-${columnName}` }),
  columnName,
  hlcTimestamp: hlc,
  deviceId: 'd1',
  encryptedValue: null,
  nonce: null,
})

describe('chunkChangesByHlc', () => {
  it('returns empty for empty input', () => {
    expect(chunkChangesByHlc([], 100)).toEqual([])
  })

  it('keeps all changes of a single HLC group in one chunk, even if oversized', () => {
    const changes = Array.from({ length: 50 }, (_, i) => makeChange('HLC-A', `c${i}`))
    const chunks = chunkChangesByHlc(changes, 10)
    expect(chunks).toHaveLength(1)
    expect(chunks[0]).toHaveLength(50)
  })

  it('splits across HLC boundaries when the soft limit would otherwise be exceeded', () => {
    const changes = [
      makeChange('HLC-A', 'c1'),
      makeChange('HLC-A', 'c2'),
      makeChange('HLC-B', 'c1'),
      makeChange('HLC-B', 'c2'),
      makeChange('HLC-C', 'c1'),
    ]
    const chunks = chunkChangesByHlc(changes, 3)
    // HLC-A (2) + HLC-B (2) = 4 > 3 → split before HLC-B
    // HLC-B (2) + HLC-C (1) = 3 ≤ 3 → both fit in the second chunk
    expect(chunks.map(c => c.length)).toEqual([2, 3])
    expect(chunks[0]!.every(c => c.hlcTimestamp === 'HLC-A')).toBe(true)
    expect(chunks[1]!.map(c => c.hlcTimestamp)).toEqual(['HLC-B', 'HLC-B', 'HLC-C'])
  })

  it('never splits a group even when the running chunk already exceeds the limit with the oversized group', () => {
    const changes = [
      makeChange('HLC-A', 'c1'),
      // HLC-B has 5 changes — larger than the soft limit of 3.
      ...Array.from({ length: 5 }, (_, i) => makeChange('HLC-B', `c${i}`)),
      makeChange('HLC-C', 'c1'),
    ]
    const chunks = chunkChangesByHlc(changes, 3)
    // HLC-A alone (1) stays, then HLC-B (5) forces a flush → own oversized chunk,
    // then HLC-C (1) goes into a final chunk.
    expect(chunks).toHaveLength(3)
    expect(chunks[0]!.map(c => c.hlcTimestamp)).toEqual(['HLC-A'])
    expect(chunks[1]!.every(c => c.hlcTimestamp === 'HLC-B')).toBe(true)
    expect(chunks[1]).toHaveLength(5)
    expect(chunks[2]!.map(c => c.hlcTimestamp)).toEqual(['HLC-C'])
  })

  it('fits multiple small groups into one chunk until the limit is reached', () => {
    const changes = [
      makeChange('HLC-A', 'c1'),
      makeChange('HLC-B', 'c1'),
      makeChange('HLC-C', 'c1'),
      makeChange('HLC-D', 'c1'),
    ]
    const chunks = chunkChangesByHlc(changes, 10)
    expect(chunks).toHaveLength(1)
    expect(chunks[0]).toHaveLength(4)
  })
})
