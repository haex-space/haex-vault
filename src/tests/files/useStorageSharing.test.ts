import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

import { invoke } from '@tauri-apps/api/core'
import {
  useStorageSharing,
  type ShareStorageBackendArgs,
  type SharedStorageBackend,
} from '@/composables/useStorageSharing'

const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>

describe('useStorageSharing', () => {
  beforeEach(() => {
    mockInvoke.mockReset()
  })

  it('shareBackend forwards args wrapped under { args } and returns the shared row', async () => {
    const shared: SharedStorageBackend = {
      id: 'shared-1',
      type: 's3',
      name: 'my-shared-backend',
      iamUserName: 'haex-scoped-user-1',
    }
    mockInvoke.mockResolvedValueOnce(shared)

    const { shareBackend } = useStorageSharing()
    const args: ShareStorageBackendArgs = {
      storageId: 'owner-1',
      spaceId: 'space-1',
      accessFlags: 0b0011,
    }

    const result = await shareBackend(args)

    expect(mockInvoke).toHaveBeenCalledTimes(1)
    expect(mockInvoke).toHaveBeenCalledWith('share_storage_backend', { args })
    expect(result).toEqual(shared)
  })

  it('revokeBackend forwards the shared row id as sharedBackendId', async () => {
    mockInvoke.mockResolvedValueOnce(undefined)

    const { revokeBackend } = useStorageSharing()
    await revokeBackend('shared-1')

    expect(mockInvoke).toHaveBeenCalledTimes(1)
    expect(mockInvoke).toHaveBeenCalledWith('revoke_storage_share', {
      sharedBackendId: 'shared-1',
    })
  })

  it('propagates the invoke rejection unchanged so callers can route on StorageError.type', async () => {
    const err = { type: 'IamAdminCredMissing', details: { storage_id: 'owner-1' } }
    mockInvoke.mockRejectedValueOnce(err)

    const { shareBackend } = useStorageSharing()
    await expect(
      shareBackend({
        storageId: 'owner-1',
        spaceId: 'space-1',
        accessFlags: 1,
      }),
    ).rejects.toBe(err)
  })
})
