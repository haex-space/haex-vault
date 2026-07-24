import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { ColumnChange } from '~/stores/sync/tableScanner'

// Mock BEFORE importing apply.ts. Vitest hoists vi.mock() automatically.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

vi.mock('@haex-space/vault-sdk', () => ({
  verifyRecordSignatureAsync: vi.fn(async () => true),
  publicKeyToDidKeyAsync: vi.fn(async (spki: string) => `did:key:z${spki}`),
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
// plain module context. The `beforeEach` in each describe wires the mocks;
// here we only need the globals to exist so a bare `useToast()` call doesn't
// throw ReferenceError while the file is being parsed.
vi.stubGlobal('useToast', () => ({ add: vi.fn() }))
vi.stubGlobal('useNuxtApp', () => ({
  $i18n: { t: (k: string) => k },
}))

// Import AFTER mocks are set up.
import { invoke } from '@tauri-apps/api/core'
import {
  verifyPulledChangesAsync,
  logRejectedChanges,
  surfaceRejectedBatch,
  type RejectedChange,
} from '~/stores/sync/orchestrator/pull/apply'

const mockInvoke = vi.mocked(invoke)

// Factory: build a ColumnChange with signature + signedBy so it passes the
// layer-1 gate. The `signedBy` value doubles as the SPKI stub — the mocked
// `publicKeyToDidKeyAsync` derives the signer DID from it deterministically.
const change = (rowPks: string, signedBy: string, hlc = '100/aa'): ColumnChange => ({
  tableName: 'haex_bookmarks',
  rowPks,
  columnName: 'title',
  hlcTimestamp: hlc,
  deviceId: 'dev-1',
  signature: 'sig-' + rowPks,
  signedBy,
})

// Composite key used for correlating verify_ucan_chain_batch results back to
// their input rows. Mirrors the `rowKey` helper inside apply.ts.
const rowKey = (c: ColumnChange) => `${c.tableName}|${c.rowPks}|${c.columnName}|${c.hlcTimestamp}`

describe('verifyPulledChangesAsync — Rust chain-verify bridge', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    // Default: every signer has one cached UCAN token in haex_ucan_tokens.
    mockDbWhere.mockResolvedValue([
      { token: 'ucan-token-stub', capability: 'space/write', expiresAt: 9999999999 },
    ])
  })

  it('sorts rows into verified and rejected buckets based on Rust outcome', async () => {
    const r1 = change('{"id":"r1"}', 'signer1')
    const r2 = change('{"id":"r2"}', 'signer2')
    const r3 = change('{"id":"r3"}', 'signer3')

    mockInvoke.mockResolvedValue([
      { rowId: rowKey(r1), tableName: r1.tableName, outcome: { kind: 'ok', rootDid: 'did:key:zroot' } },
      { rowId: rowKey(r2), tableName: r2.tableName, outcome: { kind: 'rejected', reason: 'Signature' } },
      { rowId: rowKey(r3), tableName: r3.tableName, outcome: { kind: 'ok', rootDid: 'did:key:zroot' } },
    ])

    const result = await verifyPulledChangesAsync([r1, r2, r3], 'space-123', 'did:key:zme', 'write')

    expect(result.verified).toHaveLength(2)
    expect(result.verified.map((c) => c.rowPks)).toEqual(['{"id":"r1"}', '{"id":"r3"}'])
    expect(result.rejected).toHaveLength(1)
    expect(result.rejected[0]!.tableName).toBe('haex_bookmarks')
    expect(result.rejected[0]!.reason).toBe('Signature')
  })

  it('invokes verify_ucan_chain_batch with the correct request shape', async () => {
    const r1 = change('{"id":"r1"}', 'signer1')
    mockInvoke.mockResolvedValue([
      { rowId: rowKey(r1), tableName: r1.tableName, outcome: { kind: 'ok', rootDid: 'did:key:zroot' } },
    ])

    await verifyPulledChangesAsync([r1], 'space-123', 'did:key:zme', 'write')

    expect(mockInvoke).toHaveBeenCalledTimes(1)
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

  it('preserves input order in the verified array', async () => {
    const r1 = change('{"id":"r1"}', 'signer1')
    const r2 = change('{"id":"r2"}', 'signer2')
    const r3 = change('{"id":"r3"}', 'signer3')

    // Rust responds in a scrambled order — TS must still emit verified rows
    // in the original input order so the applier sees them chronologically.
    mockInvoke.mockResolvedValue([
      { rowId: rowKey(r3), tableName: r3.tableName, outcome: { kind: 'ok', rootDid: 'did:key:z1' } },
      { rowId: rowKey(r1), tableName: r1.tableName, outcome: { kind: 'ok', rootDid: 'did:key:z1' } },
      { rowId: rowKey(r2), tableName: r2.tableName, outcome: { kind: 'ok', rootDid: 'did:key:z1' } },
    ])

    const result = await verifyPulledChangesAsync([r1, r2, r3], 'space-123', 'did:key:zme', 'write')
    expect(result.verified.map((c) => c.rowPks)).toEqual(['{"id":"r1"}', '{"id":"r2"}', '{"id":"r3"}'])
  })

  it('empty input returns empty buckets and skips the Rust call', async () => {
    const result = await verifyPulledChangesAsync([], 'space-123', 'did:key:zme', 'write')
    expect(result.verified).toEqual([])
    expect(result.rejected).toEqual([])
    expect(mockInvoke).not.toHaveBeenCalled()
  })

  it('rejects rows whose signer has no cached UCAN in haex_ucan_tokens', async () => {
    const r1 = change('{"id":"r1"}', 'signer1')
    // Local UCAN cache is empty for every signer — no local grant found.
    mockDbWhere.mockResolvedValue([])

    const result = await verifyPulledChangesAsync([r1], 'space-123', 'did:key:zme', 'write')

    // The Rust command never fires: we cannot ask it to verify a token we
    // do not possess. The row is dropped with a synthetic reason so the
    // caller (Task 6 toast) can distinguish this from a Rust-side reject.
    expect(mockInvoke).not.toHaveBeenCalled()
    expect(result.verified).toHaveLength(0)
    expect(result.rejected).toHaveLength(1)
    expect(result.rejected[0]!.reason).toBe('MissingLocalUcan')
  })

  it('rejects rows missing signature or signedBy with reason=Unsigned', async () => {
    const unsigned: ColumnChange = {
      tableName: 'haex_bookmarks',
      rowPks: '{"id":"r-unsigned"}',
      columnName: 'title',
      hlcTimestamp: '100/aa',
      deviceId: 'dev-1',
      // No signature, no signedBy — layer-0 gate.
    }
    const result = await verifyPulledChangesAsync([unsigned], 'space-123', 'did:key:zme', 'write')

    expect(mockInvoke).not.toHaveBeenCalled()
    expect(result.verified).toHaveLength(0)
    expect(result.rejected).toHaveLength(1)
    expect(result.rejected[0]!.reason).toBe('Unsigned')
  })

  it('rejects rows whose record-signature fails layer-1 verification', async () => {
    const bad = change('{"id":"r-badsig"}', 'signer1')
    const { verifyRecordSignatureAsync } = await import('@haex-space/vault-sdk')
    vi.mocked(verifyRecordSignatureAsync).mockResolvedValueOnce(false)

    const result = await verifyPulledChangesAsync([bad], 'space-123', 'did:key:zme', 'write')

    expect(mockInvoke).not.toHaveBeenCalled()
    expect(result.rejected).toHaveLength(1)
    expect(result.rejected[0]!.reason).toBe('InvalidRecordSignature')
  })

  it('assigns MissingResult when Rust does not echo the rowId back', async () => {
    const r1 = change('{"id":"r1"}', 'signer1')
    // Rust returned an empty result array despite a non-empty request.
    // This models an IPC drop or a Rust-side bug — the row must be
    // rejected (never verified) with the synthetic MissingResult reason
    // so the caller can distinguish it from a Rust-side reject.
    mockInvoke.mockResolvedValue([])

    const result = await verifyPulledChangesAsync([r1], 'space-123', 'did:key:zme', 'write')

    expect(result.verified).toHaveLength(0)
    expect(result.rejected).toHaveLength(1)
    expect(result.rejected[0]!.reason).toBe('MissingResult')
  })

  it('picks the highest-capability cached UCAN when multiple exist for a signer', async () => {
    const r1 = change('{"id":"r1"}', 'signer1')
    // Signer has both a read AND a write token cached. Without an ORDER BY
    // the picked row would be arbitrary; the fix ranks by capability so a
    // write-scoped change is served by the write token.
    mockDbWhere.mockResolvedValue([
      { token: 'read-token', capability: 'space/read', expiresAt: 9999999999 },
      { token: 'write-token', capability: 'space/write', expiresAt: 9999999999 },
    ])
    mockInvoke.mockResolvedValue([
      { rowId: rowKey(r1), tableName: r1.tableName, outcome: { kind: 'ok', rootDid: 'did:key:zroot' } },
    ])

    await verifyPulledChangesAsync([r1], 'space-123', 'did:key:zme', 'write')

    expect(mockInvoke).toHaveBeenCalledWith(
      'verify_ucan_chain_batch',
      expect.objectContaining({
        requests: expect.arrayContaining([
          expect.objectContaining({ token: 'write-token' }),
        ]),
      }),
    )
  })

  it('picks admin over write/read/invite regardless of row order', async () => {
    const r1 = change('{"id":"r1"}', 'signer1')
    // Rows returned in a scrambled order — admin must still win by rank.
    mockDbWhere.mockResolvedValue([
      { token: 'read-token', capability: 'space/read', expiresAt: 9999999999 },
      { token: 'invite-token', capability: 'space/invite', expiresAt: 9999999999 },
      { token: 'admin-token', capability: 'space/admin', expiresAt: 9999999999 },
      { token: 'write-token', capability: 'space/write', expiresAt: 9999999999 },
    ])
    mockInvoke.mockResolvedValue([
      { rowId: rowKey(r1), tableName: r1.tableName, outcome: { kind: 'ok', rootDid: 'did:key:zroot' } },
    ])

    await verifyPulledChangesAsync([r1], 'space-123', 'did:key:zme', 'write')

    expect(mockInvoke).toHaveBeenCalledWith(
      'verify_ucan_chain_batch',
      expect.objectContaining({
        requests: expect.arrayContaining([
          expect.objectContaining({ token: 'admin-token' }),
        ]),
      }),
    )
  })

  it('throws when Rust returns a malformed VerifyChainResult shape', async () => {
    const r1 = change('{"id":"r1"}', 'signer1')
    // outcome=null violates the IPC contract (must be { kind: 'ok' | 'rejected', ... }).
    // The bare `invoke<T>()` cast wouldn't catch this — the runtime guard must.
    mockInvoke.mockResolvedValue([{ rowId: rowKey(r1), outcome: null }])

    await expect(
      verifyPulledChangesAsync([r1], 'space-123', 'did:key:zme', 'write'),
    ).rejects.toThrow(/malformed shape/)
  })

  it('throws when Rust returns something that is not an array', async () => {
    const r1 = change('{"id":"r1"}', 'signer1')
    // Rust bug or IPC corruption — non-array on the wire.
    mockInvoke.mockResolvedValue({ error: 'oops' } as unknown as never)

    await expect(
      verifyPulledChangesAsync([r1], 'space-123', 'did:key:zme', 'write'),
    ).rejects.toThrow(/malformed shape/)
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

describe('logRejectedChanges — structured warn log (Task 5, log-only)', () => {
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
    // Task 6 split: aggregation moved to `surfaceRejectedBatch` so a pull
    // spanning N pages surfaces one toast, not N. Log-only helper must
    // stay log-only.
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

describe('surfaceRejectedBatch — aggregated pull-batch toast (Task 6)', () => {
  let toastAdd: ReturnType<typeof vi.fn>
  let t: ReturnType<typeof vi.fn>

  beforeEach(() => {
    // Re-stub per test so `toHaveBeenCalledTimes` is scoped to this test only.
    toastAdd = vi.fn()
    // `t` returns a rendered template so we can assert the {count} interpolation
    // reached the translator — the real Vue I18n resolves the key, but the
    // stub here just proxies args back into the title.
    t = vi.fn(
      (key: string, params?: Record<string, unknown>) =>
        params && 'count' in params ? `${key}:${params.count}` : key,
    )
    vi.stubGlobal('useToast', () => ({ add: toastAdd }))
    vi.stubGlobal('useNuxtApp', () => ({ $i18n: { t } }))
  })

  it('triggers exactly one warning toast when count > 0', () => {
    // Simulate the accumulator hand-off: streaming pull summed 2 rejections
    // across N pages and surfaces once at the end.
    surfaceRejectedBatch('space-123', 2)

    // One aggregated toast per batch — never one per row (a poisoned pull
    // of 1000 rows must not spam 1000 stacked toasts).
    expect(toastAdd).toHaveBeenCalledTimes(1)
    const arg = toastAdd.mock.calls[0]![0] as { title: string; color: string; icon: string }
    expect(arg.color).toBe('warning')
    expect(arg.icon).toBe('i-lucide-shield-alert')
    // The plural key was picked (count > 1) and {count} was forwarded to $i18n.t.
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
    // The accumulator-across-pages contract: a pull of 3 pages with 4, 6,
    // and 90 rejects surfaces once with total=100 (never three toasts).
    surfaceRejectedBatch('space-123', 100)

    expect(toastAdd).toHaveBeenCalledTimes(1)
    expect(t).toHaveBeenCalledWith('sync.verification.rowsRejectedOther', { count: 100 })
  })
})
