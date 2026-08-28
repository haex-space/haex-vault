import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { cloudStorageScopeChanged, useFileSyncStore } from '~/stores/file-sync'

const { requireDbMock } = vi.hoisted(() => ({
  requireDbMock: vi.fn(),
}))

vi.mock('~/stores/vault', () => ({
  requireDb: requireDbMock,
}))

const ownVaultCloud = {
  backendId: 'backend-a',
  bucket: 'vault',
  prefix: 'files',
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.clearAllMocks()
  vi.stubGlobal('useToast', () => ({ add: vi.fn() }))
  vi.stubGlobal('useNuxtApp', () => ({
    $i18n: { mergeLocaleMessage: vi.fn(), t: (key: string) => key },
  }))
})

describe('cloudStorageScopeChanged', () => {
  it('keeps the cache for settings-only edits', () => {
    expect(
      cloudStorageScopeChanged('cloud', ownVaultCloud, 'cloud', {
        ...ownVaultCloud,
      }),
    ).toBe(false)
  })

  it('invalidates the cache when a cloud rule changes encryption scope', () => {
    expect(
      cloudStorageScopeChanged('cloud', ownVaultCloud, 'cloud', {
        ...ownVaultCloud,
        spaceId: 'space-a',
      }),
    ).toBe(true)
  })

  it('invalidates the cache when a cloud location changes', () => {
    expect(
      cloudStorageScopeChanged('cloud', ownVaultCloud, 'cloud', {
        ...ownVaultCloud,
        backendId: 'backend-b',
      }),
    ).toBe(true)
    expect(
      cloudStorageScopeChanged('cloud', ownVaultCloud, 'cloud', {
        ...ownVaultCloud,
        prefix: 'other-files',
      }),
    ).toBe(true)
  })

  it('invalidates the cache when moving to or from a cloud provider', () => {
    expect(
      cloudStorageScopeChanged(
        'local',
        { path: '/tmp' },
        'cloud',
        ownVaultCloud,
      ),
    ).toBe(true)
    expect(
      cloudStorageScopeChanged('cloud', ownVaultCloud, 'local', {
        path: '/tmp',
      }),
    ).toBe(true)
  })
})

describe('useFileSyncStore.updateRuleAsync', () => {
  it('clears cached object keys before changing cloud encryption scope', async () => {
    const currentRule = {
      id: 'rule-a',
      sourceType: 'local',
      sourceConfig: { path: '/source' },
      targetType: 'cloud',
      targetConfig: ownVaultCloud,
    }
    const deleteWhere = vi.fn(async () => undefined)
    const updateWhere = vi.fn(async () => undefined)
    const db = {
      select: vi
        .fn()
        .mockReturnValueOnce({
          from: () => ({
            where: () => ({ limit: async () => [currentRule] }),
          }),
        })
        .mockReturnValueOnce({
          from: () => ({ all: async () => [] }),
        }),
      delete: vi.fn(() => ({ where: deleteWhere })),
      update: vi.fn(() => ({ set: () => ({ where: updateWhere }) })),
    }
    requireDbMock.mockReturnValue(db)

    const store = useFileSyncStore()
    await store.updateRuleAsync('rule-a', {
      targetConfig: { ...ownVaultCloud, spaceId: 'space-a' },
    })

    expect(db.delete).toHaveBeenCalledTimes(1)
    expect(deleteWhere).toHaveBeenCalledTimes(1)
    expect(updateWhere).toHaveBeenCalledTimes(1)
  })
})
