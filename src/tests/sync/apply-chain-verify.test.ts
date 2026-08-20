import { describe, it, expect, vi, beforeEach } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import {
  holdsSpaceCap,
  isSpaceCapValue,
  spaceCapabilitySet,
  type SpaceCap,
} from '@haex-space/ucan'
import type { ColumnChange } from '~/stores/sync/tableScanner'
import {
  verifyPulledChangesAsync,
  logRejectedChanges,
  surfaceRejectedBatch,
  type RejectedChange,
} from '~/stores/sync/orchestrator/pull/apply'

/**
 * Task 8b: `haex_ucan_tokens.capabilities` is a JSON-serialized
 * `SpaceCapabilitySet`. Build the singleton wire form the writer produces
 * so the mock rows match what the apply-side deserializer expects.
 */
const capsJson = (cap: SpaceCap): string =>
  JSON.stringify(spaceCapabilitySet()[cap](false).build())

// Mock BEFORE importing apply.ts. Vitest hoists vi.mock() automatically.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

vi.mock('@haex-space/vault-sdk', () => ({
  decryptCrdtData: vi.fn(),
}))

// Simulate the local `haex_ucan_tokens` cache. Each `where(...)` returns a
// single-token row keyed off the signer — enough to exercise the outer
// `verifyPulledChangesAsync` path where the bridge is under test.
const mockDbWhere = vi.fn()
vi.mock('~/stores/vault', () => ({
  requireDb: () => ({
    select: () => ({
      from: () => ({
        where: mockDbWhere,
      }),
    }),
  }),
}))

// Nuxt auto-imports (`useToast`, `useNuxtApp`) are not resolved by vitest —
// stub them so `surfaceRejectedBatch` can call the toast/$i18n API in a
// plain module context. Each describe block re-stubs to scope its own
// mock instances.
vi.stubGlobal('useToast', () => ({ add: vi.fn() }))
vi.stubGlobal('useNuxtApp', () => ({
  $i18n: { t: (k: string) => k },
}))

const mockInvoke = vi.mocked(invoke)

/** Composite row-key format that `verify_ucan_chain_batch` echoes back —
 *  mirrors the `rowKey` helper inside apply.ts. */
const rowKey = (c: ColumnChange) =>
  `${c.tableName}|${c.rowPks}|${c.columnName}|${c.hlcTimestamp}`

/**
 * Factory: build a `ColumnChange` carrying only the fields that survive
 * the ADR-aligned wire (no plaintext `valueBytes`). The canonical bytes
 * used by the column-sig verifier are computed post-decrypt inside
 * `applyRemoteChangesInTransactionAsync` — this file covers only
 * `verifyPulledChangesAsync` (sig-presence gate + UCAN chain).
 */
const change = (
  rowPks: string,
  authorDid: string,
  hlc = '100/aa',
): ColumnChange => ({
  tableName: 'haex_bookmarks',
  rowPks,
  columnName: 'title',
  hlcTimestamp: hlc,
  deviceId: 'dev-1',
  sig: { authorDid, sig: 'c2ln', storageClass: 'text' },
})

describe('verifyPulledChangesAsync — sig-presence + UCAN chain (Phase 1 post-review)', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    // Default: every signer has one cached UCAN token in haex_ucan_tokens.
    mockDbWhere.mockResolvedValue([
      { token: 'ucan-token-stub', capabilities: capsJson('write'), expiresAt: 9999999999 },
    ])
  })

  it('shared-space: routes UCAN check to Rust and only accepts verified rows', async () => {
    const r1 = change('{"id":"r1"}', 'did:key:zauthor1')
    const r2 = change('{"id":"r2"}', 'did:key:zauthor2')
    mockInvoke.mockResolvedValue([
      { rowId: rowKey(r1), tableName: r1.tableName, outcome: { kind: 'ok', rootDid: 'did:key:zroot' } },
      { rowId: rowKey(r2), tableName: r2.tableName, outcome: { kind: 'rejected', reason: 'Signature' } },
    ])

    const result = await verifyPulledChangesAsync([r1, r2], 'space-123', 'did:key:zme', 'write')

    expect(result.verified).toHaveLength(1)
    expect(result.verified[0]!.rowPks).toBe('{"id":"r1"}')
    expect(result.rejected).toHaveLength(1)
    expect(result.rejected[0]!.reason).toBe('Signature')
  })

  it('invokes ONLY verify_ucan_chain_batch (column-sig verify moved post-decrypt)', async () => {
    const r1 = change('{"id":"r1"}', 'did:key:zauthor1')
    mockInvoke.mockResolvedValue([
      { rowId: rowKey(r1), tableName: r1.tableName, outcome: { kind: 'ok', rootDid: 'did:key:zroot' } },
    ])

    await verifyPulledChangesAsync([r1], 'space-123', 'did:key:zme', 'write')

    // Column-sig verify is no longer part of this path — invocations
    // hitting it here would leak plaintext value bytes on the wire.
    const commands = mockInvoke.mock.calls.map((c) => c[0])
    expect(commands).toContain('verify_ucan_chain_batch')
    expect(commands).not.toContain('verify_column_sig_batch')

    expect(mockInvoke).toHaveBeenCalledWith(
      'verify_ucan_chain_batch',
      expect.objectContaining({
        requests: expect.arrayContaining([
          expect.objectContaining({
            token: 'ucan-token-stub',
            expectedSpaceId: 'space-123',
            expectedAudience: 'did:key:zme',
            capabilityNeeded: 'write',
            rowId: rowKey(r1),
            tableName: 'haex_bookmarks',
          }),
        ]),
      }),
    )
  })

  it('rejects rows missing sig with reason=Unsigned', async () => {
    // Layer-0 gate: a change without a sig never reaches Rust — reject
    // immediately with the synthetic reason so the caller can
    // distinguish it from a Rust-side reject.
    const unsigned: ColumnChange = {
      tableName: 'haex_bookmarks',
      rowPks: '{"id":"r-unsigned"}',
      columnName: 'title',
      hlcTimestamp: '100/aa',
      deviceId: 'dev-1',
      // no sig
    }
    const result = await verifyPulledChangesAsync(
      [unsigned],
      'space-123',
      'did:key:zme',
      'write',
    )

    expect(mockInvoke).not.toHaveBeenCalled()
    expect(result.verified).toHaveLength(0)
    expect(result.rejected).toHaveLength(1)
    expect(result.rejected[0]!.reason).toBe('Unsigned')
  })

  it('personal-vault (no spaceId): skips both layers entirely', async () => {
    const r1 = change('{"id":"r1"}', 'did:key:zauthor1')
    // Even an unsigned row passes through — Phase 1 doesn't sign
    // personal-vault sync.
    const r2: ColumnChange = {
      tableName: 'haex_bookmarks',
      rowPks: '{"id":"r2"}',
      columnName: 'title',
      hlcTimestamp: '100/aa',
      deviceId: 'dev-1',
    }

    const result = await verifyPulledChangesAsync([r1, r2], undefined, 'did:key:zme', 'write')

    expect(mockInvoke).not.toHaveBeenCalled()
    expect(result.verified).toHaveLength(2)
    expect(result.rejected).toHaveLength(0)
  })

  it('empty input returns empty buckets and never invokes Rust', async () => {
    const result = await verifyPulledChangesAsync([], 'space-123', 'did:key:zme', 'write')
    expect(result.verified).toEqual([])
    expect(result.rejected).toEqual([])
    expect(mockInvoke).not.toHaveBeenCalled()
  })

  it('rejects rows whose signer has no cached UCAN in haex_ucan_tokens', async () => {
    const r1 = change('{"id":"r1"}', 'did:key:zauthor1')
    mockDbWhere.mockResolvedValue([]) // no UCAN cached

    const result = await verifyPulledChangesAsync([r1], 'space-123', 'did:key:zme', 'write')

    // verify_ucan_chain_batch never fires — we cannot ask Rust to
    // verify a token we do not possess. The row is dropped with a
    // synthetic reason.
    expect(mockInvoke).not.toHaveBeenCalled()
    expect(result.verified).toHaveLength(0)
    expect(result.rejected).toHaveLength(1)
    expect(result.rejected[0]!.reason).toBe('MissingLocalUcan')
  })

  it('propagates UCAN reject reasons (Signature, Expired, …) from Rust', async () => {
    const r1 = change('{"id":"r1"}', 'did:key:zauthor1')
    const r2 = change('{"id":"r2"}', 'did:key:zauthor2')
    mockInvoke.mockResolvedValue([
      { rowId: rowKey(r1), tableName: r1.tableName, outcome: { kind: 'ok', rootDid: 'x' } },
      { rowId: rowKey(r2), tableName: r2.tableName, outcome: { kind: 'rejected', reason: 'Expired' } },
    ])

    const result = await verifyPulledChangesAsync([r1, r2], 'space-123', 'did:key:zme', 'write')

    expect(result.verified).toHaveLength(1)
    expect(result.rejected).toHaveLength(1)
    expect(result.rejected[0]!.reason).toBe('Expired')
  })

  it('assigns MissingResult when Rust drops a row silently', async () => {
    const r1 = change('{"id":"r1"}', 'did:key:zauthor1')
    // Empty results array — Rust dropped the row without an outcome entry.
    mockInvoke.mockResolvedValue([])

    const result = await verifyPulledChangesAsync([r1], 'space-123', 'did:key:zme', 'write')

    expect(result.rejected).toHaveLength(1)
    expect(result.rejected[0]!.reason).toBe('MissingResult')
  })

  it('picks the cap-matching cached UCAN with the closest expires_at', async () => {
    // W4 PR-3 semantic (Task 8b): filter cached UCANs to those whose
    // deserialized `capabilities` `SpaceCapabilitySet` holds `needed`, then
    // prefer the token closest to expiry (least-privilege intent — burn
    // down the soonest token first, keep longer-lived ones in reserve).
    // Under the orthogonal-cap model, holding `admin` no longer implies
    // holding `write`, so only the write-capable row is eligible here.
    const r1 = change('{"id":"r1"}', 'did:key:zauthor1')
    mockDbWhere.mockResolvedValue([
      { token: 'read-token',        capabilities: capsJson('read'),  expiresAt: 9999999999 },
      { token: 'admin-token',       capabilities: capsJson('admin'), expiresAt: 9999999999 },
      { token: 'write-token-later', capabilities: capsJson('write'), expiresAt: 9999999998 },
      { token: 'write-token-soon',  capabilities: capsJson('write'), expiresAt: 9999999997 },
    ])
    mockInvoke.mockResolvedValue([
      { rowId: rowKey(r1), tableName: r1.tableName, outcome: { kind: 'ok', rootDid: 'x' } },
    ])

    await verifyPulledChangesAsync([r1], 'space-123', 'did:key:zme', 'write')

    expect(mockInvoke).toHaveBeenCalledWith(
      'verify_ucan_chain_batch',
      expect.objectContaining({
        requests: expect.arrayContaining([
          expect.objectContaining({ token: 'write-token-soon' }),
        ]),
      }),
    )
  })

  it('ignores an expired cap-matching cached UCAN', async () => {
    const r1 = change('{"id":"r1"}', 'did:key:zauthor1')
    const now = Math.floor(Date.now() / 1000)
    mockDbWhere.mockResolvedValue([
      { token: 'expired-write-token', capabilities: capsJson('write'), expiresAt: now - 1 },
      { token: 'valid-write-token', capabilities: capsJson('write'), expiresAt: now + 60 },
    ])
    mockInvoke.mockResolvedValue([
      { rowId: rowKey(r1), tableName: r1.tableName, outcome: { kind: 'ok', rootDid: 'x' } },
    ])

    await verifyPulledChangesAsync([r1], 'space-123', 'did:key:zme', 'write')

    expect(mockInvoke).toHaveBeenCalledWith(
      'verify_ucan_chain_batch',
      expect.objectContaining({
        requests: expect.arrayContaining([
          expect.objectContaining({ token: 'valid-write-token' }),
        ]),
      }),
    )
  })

  it('rejects with MissingLocalUcan when no cached UCAN holds the needed cap', async () => {
    // Orthogonal semantic: an admin-only or read-only token no longer
    // covers a write need. Row must be rejected exactly as if no token
    // existed at all.
    const r1 = change('{"id":"r1"}', 'did:key:zauthor1')
    mockDbWhere.mockResolvedValue([
      { token: 'admin-token', capabilities: capsJson('admin'), expiresAt: 9999999999 },
      { token: 'read-token',  capabilities: capsJson('read'),  expiresAt: 9999999999 },
    ])

    const result = await verifyPulledChangesAsync([r1], 'space-123', 'did:key:zme', 'write')

    expect(mockInvoke).not.toHaveBeenCalled()
    expect(result.rejected[0]!.reason).toBe('MissingLocalUcan')
  })

  // -------------------------------------------------------------------------
  // Malformed local-cache rows must never grant.
  //
  // `rowHoldsCap` used to cast the parsed `capabilities` column straight to
  // `SpaceCapabilitySet` without running `isSpaceCapValue`, so it could hand
  // Rust a token off a row shape that every sibling call site rejects. These
  // tests pin the fail-closed posture: a malformed row contributes no
  // candidate token, the change is rejected with `MissingLocalUcan`, and the
  // page is NOT aborted.
  // -------------------------------------------------------------------------

  /** Shapes `isSpaceCapValue` must reject regardless of library version. */
  const malformedRowShapes: ReadonlyArray<[label: string, raw: string]> = [
    ['object instead of array', '{"cap":"write","delegatable":false}'],
    ['bare cap string (pre-8b legacy)', '"space/write"'],
    ['null', 'null'],
    ['number', '42'],
    ['unparseable JSON', 'not-json-at-all'],
    ['array of nulls', '[null]'],
    ['array of bare strings', '["write"]'],
  ]

  it.each(malformedRowShapes)(
    'malformed capabilities row (%s) never grants — rejects with MissingLocalUcan',
    async (_label, raw) => {
      const r1 = change('{"id":"r1"}', 'did:key:zauthor1')
      mockDbWhere.mockResolvedValue([
        { token: 'malformed-row-token', capabilities: raw, expiresAt: 9999999999 },
      ])

      const result = await verifyPulledChangesAsync([r1], 'space-123', 'did:key:zme', 'write')

      // The token was never offered to Rust, and the row did not sneak
      // through as verified.
      expect(mockInvoke).not.toHaveBeenCalled()
      expect(result.verified).toHaveLength(0)
      expect(result.rejected).toHaveLength(1)
      expect(result.rejected[0]!.reason).toBe('MissingLocalUcan')
    },
  )

  it('a malformed row is skipped, not fatal — a sibling well-formed row still wins', async () => {
    // Posture check: skip (like `stores/spaces/capabilities.ts` and
    // `stores/spaces/members.ts`), not throw (like `utils/auth/ucanStore.ts`).
    // One poisoned local-cache row must not wedge the page.
    const r1 = change('{"id":"r1"}', 'did:key:zauthor1')
    mockDbWhere.mockResolvedValue([
      { token: 'malformed-token', capabilities: '{"cap":"write"}', expiresAt: 9999999998 },
      { token: 'valid-write-token', capabilities: capsJson('write'), expiresAt: 9999999999 },
    ])
    mockInvoke.mockResolvedValue([
      { rowId: rowKey(r1), tableName: r1.tableName, outcome: { kind: 'ok', rootDid: 'x' } },
    ])

    const result = await verifyPulledChangesAsync([r1], 'space-123', 'did:key:zme', 'write')

    // Note the malformed row has the *closer* expiry, so a missing guard
    // would have preferred it under the least-privilege sort.
    expect(mockInvoke).toHaveBeenCalledWith(
      'verify_ucan_chain_batch',
      expect.objectContaining({
        requests: expect.arrayContaining([
          expect.objectContaining({ token: 'valid-write-token' }),
        ]),
      }),
    )
    expect(result.verified).toHaveLength(1)
    expect(result.rejected).toHaveLength(0)
  })

  it.each([
    ['entry missing delegatable', '[{"cap":"write"}]'],
    ['entry with non-boolean delegatable', '[{"cap":"write","delegatable":1}]'],
    ['entry with unknown cap name', '[{"cap":"owner","delegatable":true}]'],
    ['well-formed write entry', '[{"cap":"write","delegatable":false}]'],
  ])(
    'token selection agrees with isSpaceCapValue + holdsSpaceCap (%s)',
    async (_label, raw) => {
      // Version-independent contract: `rowHoldsCap`'s answer must be exactly
      // `isSpaceCapValue(parsed) && holdsSpaceCap(parsed, needed)`. Deriving
      // the expectation from the library rather than hardcoding it means this
      // test tightens automatically as `@haex-space/ucan` hardens its wire
      // guard — no silent divergence window while the dependency catches up.
      const parsed: unknown = JSON.parse(raw)
      const shouldGrant = isSpaceCapValue(parsed) && holdsSpaceCap(parsed, 'write')

      const r1 = change('{"id":"r1"}', 'did:key:zauthor1')
      mockDbWhere.mockResolvedValue([
        { token: 'candidate-token', capabilities: raw, expiresAt: 9999999999 },
      ])
      mockInvoke.mockResolvedValue([
        { rowId: rowKey(r1), tableName: r1.tableName, outcome: { kind: 'ok', rootDid: 'x' } },
      ])

      const result = await verifyPulledChangesAsync([r1], 'space-123', 'did:key:zme', 'write')

      if (shouldGrant) {
        expect(mockInvoke).toHaveBeenCalledWith(
          'verify_ucan_chain_batch',
          expect.objectContaining({
            requests: expect.arrayContaining([
              expect.objectContaining({ token: 'candidate-token' }),
            ]),
          }),
        )
      } else {
        expect(mockInvoke).not.toHaveBeenCalled()
        expect(result.rejected[0]!.reason).toBe('MissingLocalUcan')
      }
    },
  )

  it('preserves input order in the verified array', async () => {
    const r1 = change('{"id":"r1"}', 'did:key:zauthor1')
    const r2 = change('{"id":"r2"}', 'did:key:zauthor2')
    const r3 = change('{"id":"r3"}', 'did:key:zauthor3')
    // Rust responds in a scrambled order — TS must still emit
    // verified rows in original input order.
    mockInvoke.mockResolvedValue([
      { rowId: rowKey(r3), tableName: r3.tableName, outcome: { kind: 'ok', rootDid: 'x' } },
      { rowId: rowKey(r1), tableName: r1.tableName, outcome: { kind: 'ok', rootDid: 'x' } },
      { rowId: rowKey(r2), tableName: r2.tableName, outcome: { kind: 'ok', rootDid: 'x' } },
    ])

    const result = await verifyPulledChangesAsync([r1, r2, r3], 'space-123', 'did:key:zme', 'write')
    expect(result.verified.map((c) => c.rowPks)).toEqual([
      '{"id":"r1"}',
      '{"id":"r2"}',
      '{"id":"r3"}',
    ])
  })

  it('throws when verify_ucan_chain_batch returns a malformed shape', async () => {
    const r1 = change('{"id":"r1"}', 'did:key:zauthor1')
    // outcome=null violates the IPC contract.
    mockInvoke.mockResolvedValue([{ rowId: rowKey(r1), outcome: null }])

    await expect(
      verifyPulledChangesAsync([r1], 'space-123', 'did:key:zme', 'write'),
    ).rejects.toThrow(/verify_ucan_chain_batch returned malformed shape/)
  })

  it('throws when Rust returns something that is not an array', async () => {
    const r1 = change('{"id":"r1"}', 'did:key:zauthor1')
    mockInvoke.mockResolvedValue({ error: 'oops' } as unknown as never)

    await expect(
      verifyPulledChangesAsync([r1], 'space-123', 'did:key:zme', 'write'),
    ).rejects.toThrow(/verify_ucan_chain_batch returned malformed shape/)
  })
})

// Shared factory: rejected-row shape used by both the log-only tests
// (`logRejectedChanges`) and the toast tests (`surfaceRejectedBatch`).
const rejectedRow = (rowPks: string, reason = 'Signature'): RejectedChange => ({
  rowId: `haex_bookmarks|${rowPks}|title|100/aa`,
  tableName: 'haex_bookmarks',
  columnName: 'title',
  rowPks,
  reason,
})

describe('logRejectedChanges — structured warn log (log-only)', () => {
  let toastAdd: ReturnType<typeof vi.fn>
  let t: ReturnType<typeof vi.fn>

  beforeEach(() => {
    // Re-stub per test so the "no toast fired" regression guard below is
    // scoped to this test only. `logRejectedChanges` MUST NOT touch the
    // toast wire — the aggregated toast is a separate helper.
    toastAdd = vi.fn()
    t = vi.fn((key: string) => key)
    vi.stubGlobal('useToast', () => ({ add: toastAdd }))
    vi.stubGlobal('useNuxtApp', () => ({ $i18n: { t } }))
  })

  it('does NOT trigger a toast (regression guard — toast is caller-side now)', () => {
    logRejectedChanges(
      [rejectedRow('{"id":"r1"}'), rejectedRow('{"id":"r2"}', 'Expired')],
      { spaceId: 'space-123', backendId: 'backend-a' },
    )

    expect(toastAdd).not.toHaveBeenCalled()
    expect(t).not.toHaveBeenCalled()
  })

  it('no-ops on empty rejected list', () => {
    logRejectedChanges([], { spaceId: 'space-123', backendId: 'backend-a' })

    expect(toastAdd).not.toHaveBeenCalled()
    expect(t).not.toHaveBeenCalled()
  })
})

describe('surfaceRejectedBatch — aggregated pull-batch toast', () => {
  let toastAdd: ReturnType<typeof vi.fn>
  let t: ReturnType<typeof vi.fn>

  beforeEach(() => {
    toastAdd = vi.fn()
    t = vi.fn(
      (key: string, params?: Record<string, unknown>) =>
        params && 'count' in params ? `${key}:${params.count}` : key,
    )
    vi.stubGlobal('useToast', () => ({ add: toastAdd }))
    vi.stubGlobal('useNuxtApp', () => ({ $i18n: { t } }))
  })

  it('triggers exactly one warning toast when count > 0', () => {
    surfaceRejectedBatch('space-123', 2)

    expect(toastAdd).toHaveBeenCalledTimes(1)
    const arg = toastAdd.mock.calls[0]![0] as { title: string; color: string; icon: string }
    expect(arg.color).toBe('warning')
    expect(arg.icon).toBe('i-lucide-shield-alert')
    expect(t).toHaveBeenCalledWith('sync.verification.rowsRejectedOther', { count: 2 })
    expect(arg.title).toBe('sync.verification.rowsRejectedOther:2')
  })

  it('uses the singular key when count === 1', () => {
    surfaceRejectedBatch('space-123', 1)

    expect(toastAdd).toHaveBeenCalledTimes(1)
    expect(t).toHaveBeenCalledWith('sync.verification.rowsRejectedOne', { count: 1 })
  })

  it('does not trigger a toast when count === 0', () => {
    surfaceRejectedBatch('space-123', 0)

    expect(toastAdd).not.toHaveBeenCalled()
    expect(t).not.toHaveBeenCalled()
  })

  it('surfaces an aggregated count larger than any single page', () => {
    surfaceRejectedBatch('space-123', 100)

    expect(toastAdd).toHaveBeenCalledTimes(1)
    expect(t).toHaveBeenCalledWith('sync.verification.rowsRejectedOther', { count: 100 })
  })
})
