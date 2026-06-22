import { describe, expect, it, vi, beforeEach } from 'vitest'

import { invoke } from '@tauri-apps/api/core'
import { pullFromBackendAsync } from '@/stores/sync/orchestrator/pull'
import type { BackendSyncState } from '@/stores/sync/orchestrator/types'

// Mock the only side-effecting boundary the early-guard path could touch.
// If the no-backend guard fired *after* a destructive invoke, this mock would
// record the call — so asserting it was never called proves the guard is
// non-destructive.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}))

// `@tauri-apps/api/event` is imported at module load by pull.ts (for `emit`).
// Stub it so the import resolves under jsdom without a Tauri runtime.
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn().mockResolvedValue(undefined),
}))

// `@tauri-apps/plugin-http` exposes `fetch`; stub so no real network is hit.
vi.mock('@tauri-apps/plugin-http', () => ({
  fetch: vi.fn().mockRejectedValue(new Error('network must not be reached in this test')),
}))

// pull.ts imports the extension broadcast store at module load (used only on
// the SUCCESS path, after a real pull). It transitively pulls in the extension
// message handler, which relies on Nuxt auto-imports absent under vitest. The
// no-backend guard never reaches it, so stubbing the module is faithful.
vi.mock('~/stores/extensions/broadcast', () => ({
  useExtensionBroadcastStore: () => ({
    broadcastSyncTablesUpdated: vi.fn().mockResolvedValue(undefined),
  }),
}))

// The destructive / state-mutating Tauri commands that must NEVER run on the
// no-backend path (they would clear pending-column markers, advance cursors,
// or write CRDT rows).
const DESTRUCTIVE_COMMANDS = [
  'clear_pending_column',
  'apply_remote_changes_in_transaction',
  'clear_all_dirty_tables',
  'apply_synced_extension_migrations',
  'get_pending_columns',
]

describe('pullFromBackendAsync — no HTTP backend (owner-vault P2P case, phase 6 D3)', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockClear()
  })

  it('throws "spaceId not configured" without any destructive invoke when backends is empty', async () => {
    const backendId = 'p2p-owner-vault'
    const syncStates: BackendSyncState = {
      [backendId]: { isConnected: false, isSyncing: false, error: null },
    }

    // A pure-P2P owner vault has ZERO configured HTTP backends.
    const syncBackendsStore = {
      backends: [] as Array<{ id: string; spaceId?: string }>,
    } as unknown as ReturnType<typeof import('@/stores/sync/backends').useSyncBackendsStore>

    const syncEngineStore = {
      getSyncKeyFromDbAsync: vi.fn(),
    } as unknown as ReturnType<typeof import('@/stores/sync/engine').useSyncEngineStore>

    await expect(
      pullFromBackendAsync(backendId, 'vault-1', syncStates, syncBackendsStore, syncEngineStore),
    ).rejects.toThrow(/spaceId not configured/i)

    // Guard fired before touching any destructive command.
    const destructiveCalls = vi
      .mocked(invoke)
      .mock.calls.filter(([cmd]) => DESTRUCTIVE_COMMANDS.includes(cmd as string))
    expect(destructiveCalls).toEqual([])

    // The encryption key was never even requested (guard is the first thing).
    expect(syncEngineStore.getSyncKeyFromDbAsync).not.toHaveBeenCalled()

    // Marker state in syncStates was not corrupted: error recorded, not syncing.
    expect(syncStates[backendId]?.isSyncing).toBe(false)
  })

  it('throws when the matching backend has no spaceId, again without destructive invoke', async () => {
    const backendId = 'backend-without-space'
    const syncStates: BackendSyncState = {
      [backendId]: { isConnected: false, isSyncing: false, error: null },
    }

    // Backend exists but spaceId is absent (partial / mis-provisioned config).
    const syncBackendsStore = {
      backends: [{ id: backendId, spaceId: undefined }],
    } as unknown as ReturnType<typeof import('@/stores/sync/backends').useSyncBackendsStore>

    const syncEngineStore = {
      getSyncKeyFromDbAsync: vi.fn(),
    } as unknown as ReturnType<typeof import('@/stores/sync/engine').useSyncEngineStore>

    await expect(
      pullFromBackendAsync(backendId, 'vault-1', syncStates, syncBackendsStore, syncEngineStore),
    ).rejects.toThrow(/spaceId not configured/i)

    const destructiveCalls = vi
      .mocked(invoke)
      .mock.calls.filter(([cmd]) => DESTRUCTIVE_COMMANDS.includes(cmd as string))
    expect(destructiveCalls).toEqual([])
    expect(syncEngineStore.getSyncKeyFromDbAsync).not.toHaveBeenCalled()
  })
})
