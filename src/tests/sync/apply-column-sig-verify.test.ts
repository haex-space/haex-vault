import { describe, it, expect, vi, beforeEach } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import type { ColumnChange } from '~/stores/sync/tableScanner'
import { applyRemoteChangesInTransactionAsync } from '~/stores/sync/orchestrator/pull/apply'

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
  storageClass?: 'integer' | 'real' | 'text' | 'blob' | 'null'
}): ColumnChange => ({
  tableName: 'haex_bookmarks',
  rowPks: opts.rowPks,
  columnName: opts.columnName ?? 'title',
  hlcTimestamp: opts.hlc ?? '100/aa',
  deviceId: 'dev-1',
  encryptedValue: 'enc',
  nonce: 'nnc',
  sig: {
    authorDid: opts.authorDid,
    sig: 'c2ln',
    storageClass: opts.storageClass ?? 'text',
  },
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
    // TEXT 'Hi' canonicalises to the TEXT storage-class tag followed by its
    // UTF-8 bytes: [0x03, 0x48, 0x69] → base64 'A0hp'.
    expect(capturedSigCall!.input.changes[0]!.valueBytes).toBe('A0hp')
    expect(capturedSigCall!.input.changes[0]!.sig).toEqual({
      authorDid: 'did:key:z1',
      sig: 'c2ln',
      storageClass: 'text',
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
    expect(capturedApplyCall!.changes[0]!.sig).toEqual(r1.sig)
  })

  it('shared-space: drops every change with an ambiguous duplicate correlation key', async () => {
    const first = wireChange({ rowPks: '{"id":"r1"}', authorDid: 'did:key:z1' })
    const duplicate = { ...first, encryptedValue: 'different-ciphertext' }
    mockDecrypt.mockResolvedValueOnce({ value: 'Hi' })
    mockDecrypt.mockResolvedValueOnce({ value: 'Tampered' })

    let verifiedInputCount = -1
    let appliedChanges: Array<Record<string, unknown>> | null = null
    mockInvoke.mockImplementation(async (cmd, args) => {
      if (cmd === 'get_table_schema') {
        return [
          { name: 'id', type: 'TEXT', isPk: true },
          { name: 'title', type: 'TEXT', isPk: false },
        ]
      }
      if (cmd === 'verify_column_sig_batch') {
        const call = args as { input: { changes: unknown[] } }
        verifiedInputCount = call.input.changes.length
        return { verified: [rowKey(first)], rejected: [] }
      }
      if (cmd === 'apply_remote_changes_in_transaction') {
        appliedChanges = (args as { changes: Array<Record<string, unknown>> }).changes
        return undefined
      }
      throw new Error(`unexpected invoke: ${cmd}`)
    })

    await applyRemoteChangesInTransactionAsync(
      [first, duplicate],
      new Uint8Array(32),
      'backend-A',
      'space-123',
    )

    expect(verifiedInputCount).toBe(1)
    expect(appliedChanges).toEqual([])
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

  it('NULL column with valid sig reaches the verifier', async () => {
    // Regression guard for the reviewer's Ship-Blocker #1: NULL is a valid
    // signed value and must reach the verifier. Its canonical encoding now
    // contains the explicit NULL storage-class tag.
    const r1 = wireChange({
      rowPks: '{"id":"r1"}',
      authorDid: 'did:key:z1',
      storageClass: 'null',
    })
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
    // NULL → the bare NULL storage-class tag [0x05] → base64 'BQ=='. It is
    // deliberately NOT the empty string: an empty body is what NULL,
    // TEXT('') and BLOB([]) used to share, and that collision let one
    // signature verify against all three values.
    expect(capturedSigCall!.input.changes[0]!.valueBytes).toBe('BQ==')
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
