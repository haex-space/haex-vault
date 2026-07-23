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

// Import AFTER mocks are set up.
import { invoke } from '@tauri-apps/api/core'
import { verifyPulledChangesAsync } from '~/stores/sync/orchestrator/pull/apply'

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
})
