import { describe, it, expect, vi, beforeEach } from 'vitest'

const revokeBackendMock = vi.fn<(sharedBackendId: string) => Promise<void>>()

vi.mock('@/composables/useStorageSharing', () => ({
  useStorageSharing: () => ({
    shareBackend: vi.fn(),
    revokeBackend: revokeBackendMock,
  }),
}))

import {
  useSpaceItemDelete,
  SpaceItemNotSupportedError,
  __resetSpaceItemDeleteRegistryForTests,
  type SpaceItemDeleteContext,
} from '@/composables/useSpaceItemDelete'

describe('useSpaceItemDelete', () => {
  beforeEach(() => {
    revokeBackendMock.mockReset()
    revokeBackendMock.mockResolvedValue(undefined)
    __resetSpaceItemDeleteRegistryForTests()
  })

  it('registers a handler and dispatches to it', async () => {
    const { registerHandler, deleteItem, hasHandler } = useSpaceItemDelete()

    const handler = vi.fn().mockResolvedValue(undefined)
    registerHandler('shared_file', handler)

    expect(hasHandler('shared_file')).toBe(true)

    const ctx: SpaceItemDeleteContext = {
      itemType: 'shared_file',
      itemId: 'file-42',
      spaceId: 'space-1',
      label: 'notes.md',
    }
    await deleteItem(ctx)

    expect(handler).toHaveBeenCalledTimes(1)
    expect(handler).toHaveBeenCalledWith(ctx)
  })

  it('throws SpaceItemNotSupportedError when no handler is registered', async () => {
    const { deleteItem, hasHandler } = useSpaceItemDelete()

    expect(hasHandler('extension_grant')).toBe(false)

    const promise = deleteItem({
      itemType: 'extension_grant',
      itemId: 'grant-1',
      spaceId: 'space-1',
    })

    await expect(promise).rejects.toBeInstanceOf(SpaceItemNotSupportedError)
    await expect(promise).rejects.toMatchObject({
      code: 'not_supported',
      itemType: 'extension_grant',
    })
  })

  it('registers shared_cloud_storage by default and calls revokeBackend', async () => {
    const { deleteItem, hasHandler } = useSpaceItemDelete()

    expect(hasHandler('shared_cloud_storage')).toBe(true)

    await deleteItem({
      itemType: 'shared_cloud_storage',
      itemId: 'shared-backend-7',
      spaceId: 'space-1',
    })

    expect(revokeBackendMock).toHaveBeenCalledTimes(1)
    expect(revokeBackendMock).toHaveBeenCalledWith('shared-backend-7')
  })

  it('registering the same type twice replaces the handler and warns', async () => {
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
    try {
      const { registerHandler, deleteItem } = useSpaceItemDelete()

      const first = vi.fn().mockResolvedValue(undefined)
      const second = vi.fn().mockResolvedValue(undefined)

      registerHandler('shared_file', first)
      registerHandler('shared_file', second)

      expect(warnSpy).toHaveBeenCalledTimes(1)
      expect(warnSpy.mock.calls[0]?.[0]).toContain('shared_file')

      await deleteItem({
        itemType: 'shared_file',
        itemId: 'file-1',
        spaceId: 'space-1',
      })

      expect(first).not.toHaveBeenCalled()
      expect(second).toHaveBeenCalledTimes(1)
    } finally {
      warnSpy.mockRestore()
    }
  })

  it('propagates the underlying handler error unchanged', async () => {
    const boom = new Error('revoke failed')
    revokeBackendMock.mockRejectedValueOnce(boom)

    const { deleteItem } = useSpaceItemDelete()
    await expect(
      deleteItem({
        itemType: 'shared_cloud_storage',
        itemId: 'shared-backend-9',
        spaceId: 'space-1',
      }),
    ).rejects.toBe(boom)
  })
})
