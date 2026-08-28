import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { cloudStorageScopeChanged, useFileSyncStore } from '~/stores/file-sync'

const { invokeMock, requireDbMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  requireDbMock: vi.fn(),
}))

vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }))

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
  it('stops the old loop, clears cached object keys, then starts the new scope', async () => {
    const currentRule = {
      id: 'rule-a',
      sourceType: 'local',
      sourceConfig: { path: '/source' },
      targetType: 'cloud',
      targetConfig: ownVaultCloud,
      enabled: true,
    }
    const calls: string[] = []
    const deleteWhere = vi.fn(async () => { calls.push('delete') })
    const updateWhere = vi.fn(async () => { calls.push('update') })
    const db = {
      select: vi
        .fn()
        .mockReturnValueOnce({
          from: () => ({
            where: () => ({ limit: async () => [currentRule] }),
          }),
        })
        .mockReturnValueOnce({
          from: () => ({ all: async () => [currentRule] }),
        }),
      delete: vi.fn(() => ({ where: deleteWhere })),
      update: vi.fn(() => ({ set: () => ({ where: updateWhere }) })),
    }
    requireDbMock.mockReturnValue(db)
    invokeMock.mockImplementation(async (command: string) => {
      calls.push(command)
      return command === 'file_sync_get_log' ? [] : undefined
    })

    const store = useFileSyncStore()
    await store.updateRuleAsync('rule-a', {
      targetConfig: { ...ownVaultCloud, spaceId: 'space-a' },
    })

    expect(db.delete).toHaveBeenCalledTimes(1)
    expect(deleteWhere).toHaveBeenCalledTimes(1)
    expect(updateWhere).toHaveBeenCalledTimes(1)
    expect(calls.indexOf('file_sync_stop_rule')).toBeLessThan(calls.indexOf('delete'))
    expect(calls.indexOf('delete')).toBeLessThan(calls.indexOf('update'))
    expect(calls.indexOf('update')).toBeLessThan(calls.indexOf('file_sync_start_rule'))
  })
})

describe('useFileSyncStore.restartRulesUsingBackendAsync', () => {
  it('clears object keys before restarting a rule after its backend changes', async () => {
    const rule = {
      id: 'rule-a',
      sourceType: 'cloud',
      sourceConfig: ownVaultCloud,
      targetType: 'local',
      targetConfig: { path: '/target' },
      enabled: true,
    }
    const calls: string[] = []
    const deleteWhere = vi.fn(async () => { calls.push('delete') })
    const db = {
      select: vi.fn(() => ({ from: () => ({ all: async () => [rule] }) })),
      delete: vi.fn(() => ({ where: deleteWhere })),
    }
    requireDbMock.mockReturnValue(db)
    invokeMock.mockImplementation(async (command: string) => {
      calls.push(command)
      return command === 'file_sync_get_log' ? [] : undefined
    })

    const restarted = await useFileSyncStore().restartRulesUsingBackendAsync('backend-a')

    expect(restarted).toBe(1)
    expect(calls.indexOf('file_sync_stop_rule')).toBeLessThan(calls.indexOf('delete'))
    expect(calls.indexOf('delete')).toBeLessThan(calls.indexOf('file_sync_start_rule'))
  })
})
