import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { ColumnChange } from '~/stores/sync/tableScanner'

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

// Import AFTER mocks are set up.
import { invoke } from '@tauri-apps/api/core'
import {
  verifyPulledChangesAsync,
  logRejectedChanges,
  surfaceRejectedBatch,
  type RejectedChange,
} from '~/stores/sync/orchestrator/pull/apply'

const mockInvoke = vi.mocked(invoke)

/** Composite row-key format that `verify_column_sig_batch` echoes back on
 *  both `verified` and `rejected` — mirrors `compose_row_key` in
 *  `src-tauri/src/crdt/column_sig/commands.rs`. */
const rowKey = (c: ColumnChange) =>
  `${c.tableName}|${c.rowPks}|${c.columnName}|${c.hlcTimestamp}`

/**
 * Factory: build a `ColumnChange` with the new Phase-1 wire fields —
 * a per-column `sig` and canonical `valueBytes`. `authorDid` doubles as
 * the signer DID that the UCAN layer will look up in `haex_ucan_tokens`.
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
  valueBytes: 'AAAAAAAAAA==', // arbitrary base64 stub — Rust verifier is mocked
  sig: { authorDid, sig: 'c2ln' },
})

/**
 * Configure the two Rust invocations for the happy shared-space path:
 *   1. `verify_column_sig_batch` accepts every row in `sigOk`.
 *   2. `verify_ucan_chain_batch` accepts every row in `ucanOk`.
 * Any change not listed as OK is silently dropped by the mock (models
 * `MissingResult`). Callers pass row-ids as they will appear in the
 * `rowKey`/`rowId` payloads.
 */
const mockRustHappyPath = (opts: {
  sigOk: string[]
  ucanOk: string[]
  sigRejects?: Array<{ rowKey: string; reason: string }>
}) => {
  mockInvoke.mockImplementation(async (cmd, _args) => {
    if (cmd === 'verify_column_sig_batch') {
      return { verified: opts.sigOk, rejected: opts.sigRejects ?? [] }
    }
    if (cmd === 'verify_ucan_chain_batch') {
      return opts.ucanOk.map((rowId) => ({
        rowId,
        tableName: 'haex_bookmarks',
        outcome: { kind: 'ok', rootDid: 'did:key:zroot' },
      }))
    }
    throw new Error(`unexpected invoke: ${cmd}`)
  })
}

describe('verifyPulledChangesAsync — Phase 1 column-sig + UCAN chain bridge', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    // Default: every signer has one cached UCAN token in haex_ucan_tokens.
    mockDbWhere.mockResolvedValue([
      { token: 'ucan-token-stub', capability: 'space/write', expiresAt: 9999999999 },
    ])
  })

  it('shared-space: routes column-sig verify to Rust and only applies verified rows', async () => {
    const r1 = change('{"id":"r1"}', 'did:key:zauthor1')
    const r2 = change('{"id":"r2"}', 'did:key:zauthor2')
    mockRustHappyPath({
      sigOk: [rowKey(r1)],
      ucanOk: [rowKey(r1)],
      sigRejects: [{ rowKey: rowKey(r2), reason: 'InvalidSignature' }],
    })

    const result = await verifyPulledChangesAsync([r1, r2], 'space-123', 'did:key:zme', 'write')

    expect(result.verified).toHaveLength(1)
    expect(result.verified[0]!.rowPks).toBe('{"id":"r1"}')
    expect(result.rejected).toHaveLength(1)
    expect(result.rejected[0]!.reason).toBe('InvalidSignature')
  })

  it('invokes verify_column_sig_batch with the batch input shape', async () => {
    const r1 = change('{"id":"r1"}', 'did:key:zauthor1')
    mockRustHappyPath({ sigOk: [rowKey(r1)], ucanOk: [rowKey(r1)] })

    await verifyPulledChangesAsync([r1], 'space-123', 'did:key:zme', 'write')

    expect(mockInvoke).toHaveBeenCalledWith(
      'verify_column_sig_batch',
      expect.objectContaining({
        input: expect.objectContaining({
          expectedSpaceId: 'space-123',
          changes: expect.arrayContaining([
            expect.objectContaining({
              tableName: 'haex_bookmarks',
              rowPks: '{"id":"r1"}',
              columnName: 'title',
              hlcTimestamp: '100/aa',
              valueBytes: 'AAAAAAAAAA==',
              sig: { authorDid: 'did:key:zauthor1', sig: 'c2ln' },
            }),
          ]),
        }),
      }),
    )
  })

  it('rejects rows missing sig or valueBytes with reason=Unsigned', async () => {
    // Layer-0 gate: an unsigned row (or one without canonical value
    // bytes) never reaches Rust — reject immediately with the synthetic
    // reason so the caller can distinguish it from a Rust-side reject.
    const unsigned: ColumnChange = {
      tableName: 'haex_bookmarks',
      rowPks: '{"id":"r-unsigned"}',
      columnName: 'title',
      hlcTimestamp: '100/aa',
      deviceId: 'dev-1',
      valueBytes: '',
      // sig omitted
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

  it('propagates the 5-string Rust reason vocabulary verbatim', async () => {
    // A single Rust reject drives every reason path — reason string
    // ends up on `RejectedChange.reason` untouched.
    const bad = change('{"id":"r-bad"}', 'did:key:zauthor-bad')
    mockRustHappyPath({
      sigOk: [],
      ucanOk: [],
      sigRejects: [{ rowKey: rowKey(bad), reason: 'MalformedDid' }],
    })

    const result = await verifyPulledChangesAsync([bad], 'space-123', 'did:key:zme', 'write')

    expect(result.rejected).toHaveLength(1)
    expect(result.rejected[0]!.reason).toBe('MalformedDid')
  })

  it('personal-vault (no spaceId): skips both column-sig and UCAN layers', async () => {
    const r1 = change('{"id":"r1"}', 'did:key:zauthor1')
    const r2: ColumnChange = {
      tableName: 'haex_bookmarks',
      rowPks: '{"id":"r2"}',
      columnName: 'title',
      hlcTimestamp: '100/aa',
      deviceId: 'dev-1',
      valueBytes: '',
      // Even unsigned rows pass through — Phase 1 does not sign personal-vault sync.
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
    mockRustHappyPath({ sigOk: [rowKey(r1)], ucanOk: [rowKey(r1)] })

    const result = await verifyPulledChangesAsync([r1], 'space-123', 'did:key:zme', 'write')

    // verify_column_sig_batch runs (layer-1 passes), verify_ucan_chain_batch
    // is skipped because we have no token to hand it.
    const calls = mockInvoke.mock.calls.map((c) => c[0])
    expect(calls).toContain('verify_column_sig_batch')
    expect(calls).not.toContain('verify_ucan_chain_batch')
    expect(result.verified).toHaveLength(0)
    expect(result.rejected).toHaveLength(1)
    expect(result.rejected[0]!.reason).toBe('MissingLocalUcan')
  })

  it('propagates UCAN reject reasons (Signature, Expired, …) from Rust', async () => {
    const r1 = change('{"id":"r1"}', 'did:key:zauthor1')
    const r2 = change('{"id":"r2"}', 'did:key:zauthor2')
    mockInvoke.mockImplementation(async (cmd) => {
      if (cmd === 'verify_column_sig_batch') {
        return { verified: [rowKey(r1), rowKey(r2)], rejected: [] }
      }
      if (cmd === 'verify_ucan_chain_batch') {
        return [
          { rowId: rowKey(r1), tableName: r1.tableName, outcome: { kind: 'ok', rootDid: 'x' } },
          { rowId: rowKey(r2), tableName: r2.tableName, outcome: { kind: 'rejected', reason: 'Expired' } },
        ]
      }
      throw new Error(`unexpected invoke: ${cmd}`)
    })

    const result = await verifyPulledChangesAsync([r1, r2], 'space-123', 'did:key:zme', 'write')

    expect(result.verified).toHaveLength(1)
    expect(result.rejected).toHaveLength(1)
    expect(result.rejected[0]!.reason).toBe('Expired')
  })

  it('assigns MissingResult when Rust drops a row silently in either layer', async () => {
    const r1 = change('{"id":"r1"}', 'did:key:zauthor1')
    // Rust returned neither verified nor rejected for r1 in the column-sig
    // layer — synthetic MissingResult must fire.
    mockInvoke.mockImplementation(async (cmd) => {
      if (cmd === 'verify_column_sig_batch') return { verified: [], rejected: [] }
      throw new Error(`unexpected invoke: ${cmd}`)
    })

    const result = await verifyPulledChangesAsync([r1], 'space-123', 'did:key:zme', 'write')

    expect(result.rejected).toHaveLength(1)
    expect(result.rejected[0]!.reason).toBe('MissingResult')
  })

  it('picks the highest-capability cached UCAN when multiple exist for a signer', async () => {
    const r1 = change('{"id":"r1"}', 'did:key:zauthor1')
    mockDbWhere.mockResolvedValue([
      { token: 'read-token', capability: 'space/read', expiresAt: 9999999999 },
      { token: 'invite-token', capability: 'space/invite', expiresAt: 9999999999 },
      { token: 'admin-token', capability: 'space/admin', expiresAt: 9999999999 },
      { token: 'write-token', capability: 'space/write', expiresAt: 9999999999 },
    ])
    mockRustHappyPath({ sigOk: [rowKey(r1)], ucanOk: [rowKey(r1)] })

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

  it('preserves input order in the verified array', async () => {
    const r1 = change('{"id":"r1"}', 'did:key:zauthor1')
    const r2 = change('{"id":"r2"}', 'did:key:zauthor2')
    const r3 = change('{"id":"r3"}', 'did:key:zauthor3')
    mockInvoke.mockImplementation(async (cmd) => {
      if (cmd === 'verify_column_sig_batch') {
        return { verified: [rowKey(r1), rowKey(r2), rowKey(r3)], rejected: [] }
      }
      if (cmd === 'verify_ucan_chain_batch') {
        // Rust responds in a scrambled order — TS must still emit
        // verified rows in original input order.
        return [
          { rowId: rowKey(r3), tableName: r3.tableName, outcome: { kind: 'ok', rootDid: 'x' } },
          { rowId: rowKey(r1), tableName: r1.tableName, outcome: { kind: 'ok', rootDid: 'x' } },
          { rowId: rowKey(r2), tableName: r2.tableName, outcome: { kind: 'ok', rootDid: 'x' } },
        ]
      }
      throw new Error(`unexpected invoke: ${cmd}`)
    })

    const result = await verifyPulledChangesAsync([r1, r2, r3], 'space-123', 'did:key:zme', 'write')
    expect(result.verified.map((c) => c.rowPks)).toEqual([
      '{"id":"r1"}',
      '{"id":"r2"}',
      '{"id":"r3"}',
    ])
  })

  it('throws when verify_column_sig_batch returns a malformed shape', async () => {
    const r1 = change('{"id":"r1"}', 'did:key:zauthor1')
    mockInvoke.mockImplementation(async (cmd) => {
      if (cmd === 'verify_column_sig_batch') return { verified: 'bogus', rejected: [] }
      throw new Error(`unexpected invoke: ${cmd}`)
    })

    await expect(
      verifyPulledChangesAsync([r1], 'space-123', 'did:key:zme', 'write'),
    ).rejects.toThrow(/verify_column_sig_batch returned malformed shape/)
  })

  it('throws when verify_ucan_chain_batch returns a malformed shape', async () => {
    const r1 = change('{"id":"r1"}', 'did:key:zauthor1')
    mockInvoke.mockImplementation(async (cmd) => {
      if (cmd === 'verify_column_sig_batch') return { verified: [rowKey(r1)], rejected: [] }
      // UCAN layer: outcome=null violates the IPC contract.
      if (cmd === 'verify_ucan_chain_batch') return [{ rowId: rowKey(r1), outcome: null }]
      throw new Error(`unexpected invoke: ${cmd}`)
    })

    await expect(
      verifyPulledChangesAsync([r1], 'space-123', 'did:key:zme', 'write'),
    ).rejects.toThrow(/verify_ucan_chain_batch returned malformed shape/)
  })
})

// Shared factory: rejected-row shape used by both the log-only tests
// (`logRejectedChanges`) and the toast tests (`surfaceRejectedBatch`).
const rejectedRow = (rowPks: string, reason = 'InvalidSignature'): RejectedChange => ({
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
