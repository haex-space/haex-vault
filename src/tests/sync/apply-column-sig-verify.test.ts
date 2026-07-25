import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { ColumnChange } from '~/stores/sync/tableScanner'

// Mock BEFORE importing apply.ts. Vitest hoists vi.mock() automatically.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

// Decrypt returns { value } — mirrors the `encryptCrdtData` wrapping in
// tableScanner.ts. Every test controls what plaintext comes out via the
// mock's per-call return value. `vi.hoisted` is required because
// vitest hoists `vi.mock` factories above the top-level `const`.
const { mockDecrypt } = vi.hoisted(() => ({ mockDecrypt: vi.fn() }))
vi.mock('@haex-space/vault-sdk', () => ({
  decryptCrdtData: mockDecrypt,
}))

// Not exercised in this file, but required by the vault-store import chain.
vi.mock('~/stores/vault', () => ({
  requireDb: () => ({
    select: () => ({ from: () => ({ where: () => Promise.resolve([]) }) }),
  }),
}))

vi.stubGlobal('useToast', () => ({ add: vi.fn() }))
vi.stubGlobal('useNuxtApp', () => ({ $i18n: { t: (k: string) => k } }))

import { invoke } from '@tauri-apps/api/core'
import { applyRemoteChangesInTransactionAsync } from '~/stores/sync/orchestrator/pull/apply'

const mockInvoke = vi.mocked(invoke)

/**
 * Wire-shape ColumnChange for a shared-space pull. `sig` present +
 * `encryptedValue`/`nonce` so the decrypt loop runs. NO `valueBytes` —
 * the receiver computes it locally after decrypt (ADR 0002 §2
 * confidentiality guarantee).
 */
const wireChange = (opts: {
  rowPks: string
  columnName?: string
  authorDid: string
  hlc?: string
}): ColumnChange => ({
  tableName: 'haex_bookmarks',
  rowPks: opts.rowPks,
  columnName: opts.columnName ?? 'title',
  hlcTimestamp: opts.hlc ?? '100/aa',
  deviceId: 'dev-1',
  encryptedValue: 'enc',
  nonce: 'nnc',
  sig: { authorDid: opts.authorDid, sig: 'c2ln' },
})

/** Composite row_key mirroring the Rust side. */
const rowKey = (c: ColumnChange) =>
  `${c.tableName}|${c.rowPks}|${c.columnName}|${c.hlcTimestamp}`

describe('applyRemoteChangesInTransactionAsync — post-decrypt column-sig verify', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('shared-space: canonicalises decrypted value locally + invokes verify_column_sig_batch', async () => {
    const r1 = wireChange({ rowPks: '{"id":"r1"}', authorDid: 'did:key:z1' })
    // Decrypt returns the plaintext title. The mocked verifier just
    // approves everything — we're testing that the wire payload was
    // built with the *local* canonical bytes, not shipped ones.
    mockDecrypt.mockResolvedValue({ value: 'Hi' })

    let capturedSigCall:
      | { input: { changes: Array<Record<string, unknown>>; expectedSpaceId: string } }
      | null = null

    mockInvoke.mockImplementation(async (cmd, args) => {
      if (cmd === 'get_table_schema') {
        return [
          { name: 'id', type: 'TEXT', isPk: true },
          { name: 'title', type: 'TEXT', isPk: false },
        ]
      }
      if (cmd === 'verify_column_sig_batch') {
        capturedSigCall = args as typeof capturedSigCall
        return { verified: [rowKey(r1)], rejected: [] }
      }
      if (cmd === 'apply_remote_changes_in_transaction') return undefined
      throw new Error(`unexpected invoke: ${cmd}`)
    })

    await applyRemoteChangesInTransactionAsync(
      [r1],
      new Uint8Array(32),
      'backend-A',
      'space-123',
    )

    expect(capturedSigCall).not.toBeNull()
    expect(capturedSigCall!.input.expectedSpaceId).toBe('space-123')
    expect(capturedSigCall!.input.changes).toHaveLength(1)
    // TEXT 'Hi' canonicalises to UTF-8 [0x48, 0x69] → base64 'SGk='
    expect(capturedSigCall!.input.changes[0]!.valueBytes).toBe('SGk=')
    expect(capturedSigCall!.input.changes[0]!.sig).toEqual({
      authorDid: 'did:key:z1',
      sig: 'c2ln',
    })
  })

  it('shared-space: filters out rows the Rust verifier rejected', async () => {
    const r1 = wireChange({ rowPks: '{"id":"r1"}', authorDid: 'did:key:z1' })
    const r2 = wireChange({ rowPks: '{"id":"r2"}', authorDid: 'did:key:z2' })
    mockDecrypt.mockResolvedValueOnce({ value: 'Hi' })
    mockDecrypt.mockResolvedValueOnce({ value: 'Bye' })

    let capturedApplyCall:
      | { changes: Array<Record<string, unknown>>; backendId: string; maxHlc: string }
      | null = null

    mockInvoke.mockImplementation(async (cmd, args) => {
      if (cmd === 'get_table_schema') {
        return [
          { name: 'id', type: 'TEXT', isPk: true },
          { name: 'title', type: 'TEXT', isPk: false },
        ]
      }
      if (cmd === 'verify_column_sig_batch') {
        return {
          verified: [rowKey(r1)],
          rejected: [{ rowKey: rowKey(r2), reason: 'InvalidSignature' }],
        }
      }
      if (cmd === 'apply_remote_changes_in_transaction') {
        capturedApplyCall = args as typeof capturedApplyCall
        return undefined
      }
      throw new Error(`unexpected invoke: ${cmd}`)
    })

    await applyRemoteChangesInTransactionAsync(
      [r1, r2],
      new Uint8Array(32),
      'backend-A',
      'space-123',
    )

    // Only the verified row survives into the apply invocation.
    expect(capturedApplyCall).not.toBeNull()
    expect(capturedApplyCall!.changes).toHaveLength(1)
    expect(capturedApplyCall!.changes[0]!.rowPks).toBe('{"id":"r1"}')
    // The wire `sig` field is stripped before the Rust boundary — the
    // apply command consumes only the decrypted value + coordinates.
    expect(capturedApplyCall!.changes[0]!).not.toHaveProperty('sig')
  })

  it('personal-vault (no spaceId): does not invoke verify_column_sig_batch', async () => {
    const r1 = wireChange({ rowPks: '{"id":"r1"}', authorDid: 'did:key:z1' })
    mockDecrypt.mockResolvedValue({ value: 'Hi' })

    const commands: string[] = []
    mockInvoke.mockImplementation(async (cmd) => {
      commands.push(cmd)
      if (cmd === 'get_table_schema') {
        return [
          { name: 'id', type: 'TEXT', isPk: true },
          { name: 'title', type: 'TEXT', isPk: false },
        ]
      }
      if (cmd === 'apply_remote_changes_in_transaction') return undefined
      throw new Error(`unexpected invoke: ${cmd}`)
    })

    await applyRemoteChangesInTransactionAsync(
      [r1],
      new Uint8Array(32),
      'backend-A',
      // no spaceId
    )

    expect(commands).toContain('apply_remote_changes_in_transaction')
    expect(commands).not.toContain('verify_column_sig_batch')
  })

  it('NULL column with valid sig reaches the verifier (regression: empty valueBytes is legitimate)', async () => {
    // Regression guard for the reviewer's Ship-Blocker #1: an empty
    // canonical byte string is the canonical form of NULL. Before the
    // refactor, a Layer-0 gate `!valueBytes` would have false-rejected
    // this as `Unsigned`. Post-refactor, we compute bytes locally and
    // NULL naturally yields empty bytes — must reach the verifier.
    const r1 = wireChange({ rowPks: '{"id":"r1"}', authorDid: 'did:key:z1' })
    mockDecrypt.mockResolvedValue({ value: null })

    let capturedSigCall:
      | { input: { changes: Array<Record<string, unknown>> } }
      | null = null

    mockInvoke.mockImplementation(async (cmd, args) => {
      if (cmd === 'get_table_schema') {
        return [
          { name: 'id', type: 'TEXT', isPk: true },
          { name: 'title', type: 'TEXT', isPk: false },
        ]
      }
      if (cmd === 'verify_column_sig_batch') {
        capturedSigCall = args as typeof capturedSigCall
        return { verified: [rowKey(r1)], rejected: [] }
      }
      if (cmd === 'apply_remote_changes_in_transaction') return undefined
      throw new Error(`unexpected invoke: ${cmd}`)
    })

    await applyRemoteChangesInTransactionAsync(
      [r1],
      new Uint8Array(32),
      'backend-A',
      'space-123',
    )

    expect(capturedSigCall).not.toBeNull()
    expect(capturedSigCall!.input.changes).toHaveLength(1)
    // NULL → empty bytes → empty base64 string. The verifier receives
    // this and can compare against the sig's preimage (which was signed
    // over the same empty bytes on the writer side).
    expect(capturedSigCall!.input.changes[0]!.valueBytes).toBe('')
  })

  it('empty batch is a no-op', async () => {
    await applyRemoteChangesInTransactionAsync(
      [],
      new Uint8Array(32),
      'backend-A',
      'space-123',
    )
    // Apply still fires with an empty change list; verify does not.
    const commands = mockInvoke.mock.calls.map((c) => c[0])
    expect(commands).not.toContain('verify_column_sig_batch')
  })
})
