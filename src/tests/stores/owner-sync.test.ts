import { describe, expect, it, vi, beforeEach } from 'vitest'

import { invoke } from '@tauri-apps/api/core'
import {
  startOwnerSyncAsync,
  stopOwnerSyncAsync,
  forceOwnerSyncAsync,
  ownerSyncAutostartEnabled,
} from '@/stores/peer-storage/owner-sync'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}))

describe('owner-sync command wrappers', () => {
  beforeEach(() => { vi.mocked(invoke).mockClear() })

  it('startOwnerSyncAsync invokes owner_sync_start with no args', async () => {
    await startOwnerSyncAsync()
    expect(invoke).toHaveBeenCalledTimes(1)
    expect(invoke).toHaveBeenCalledWith('owner_sync_start')
  })
  it('stopOwnerSyncAsync invokes owner_sync_stop', async () => {
    await stopOwnerSyncAsync()
    expect(invoke).toHaveBeenCalledTimes(1)
    expect(invoke).toHaveBeenCalledWith('owner_sync_stop')
  })
  it('forceOwnerSyncAsync invokes owner_sync_force', async () => {
    await forceOwnerSyncAsync()
    expect(invoke).toHaveBeenCalledTimes(1)
    expect(invoke).toHaveBeenCalledWith('owner_sync_force')
  })
})

describe('ownerSyncAutostartEnabled (default-ON)', () => {
  it('missing setting → enabled', () => {
    expect(ownerSyncAutostartEnabled(undefined)).toBe(true)
  })
  it("'false' → disabled", () => {
    expect(ownerSyncAutostartEnabled('false')).toBe(false)
  })
  it("'true' → enabled", () => {
    expect(ownerSyncAutostartEnabled('true')).toBe(true)
  })
})
