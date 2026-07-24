import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'

// Vault store is a Nuxt auto-import; stub before the store module is loaded so
// `useVaultStore()` resolves inside setup(). The stub is per-test so we can
// swap the drizzle mock between cases.
type Row = { value: string | null }
type DrizzleMock = {
  select: () => { from: () => { where: () => { limit: (n: number) => Promise<Row[]> } } }
  update: () => { set: () => { where: () => Promise<void> } }
  insert: () => { values: (row: unknown) => Promise<void> }
}

const insertSpy = vi.fn(async (_row: unknown) => undefined)
const updateSpy = vi.fn(async () => undefined)
let selectQueue: Row[][] = []

const buildDrizzleMock = (): DrizzleMock => ({
  select: () => ({
    from: () => ({
      where: () => ({
        limit: async () => selectQueue.shift() ?? [],
      }),
    }),
  }),
  update: () => ({
    set: () => ({
      where: async () => {
        await updateSpy()
      },
    }),
  }),
  insert: () => ({
    values: async (row: unknown) => {
      await insertSpy(row)
    },
  }),
})

let drizzleMock: DrizzleMock | null = buildDrizzleMock()

const useVaultStoreStub = () => ({
  currentVault: { drizzle: drizzleMock },
})

beforeEach(() => {
  setActivePinia(createPinia())
  vi.clearAllMocks()
  selectQueue = []
  drizzleMock = buildDrizzleMock()
  vi.stubGlobal('useVaultStore', useVaultStoreStub)
})

afterEach(() => {
  vi.unstubAllGlobals()
})

// Import after the global stub is registered so the store's `useVaultStore()`
// call at setup-time picks up the stub. Vitest re-evaluates modules per test
// file, so a top-level import is safe here.
import {
  useSyncConfigStore,
  DEFAULT_SYNC_CONFIG,
  MAX_UCAN_CHAIN_DEPTH_DEFAULT,
  MAX_UCAN_CHAIN_DEPTH_MIN,
  MAX_UCAN_CHAIN_DEPTH_MAX,
  parseMaxUcanChainDepth,
} from '~/stores/sync/config'

describe('parseMaxUcanChainDepth', () => {
  it('returns default for null / undefined', () => {
    expect(parseMaxUcanChainDepth(null)).toBe(MAX_UCAN_CHAIN_DEPTH_DEFAULT)
    expect(parseMaxUcanChainDepth(undefined)).toBe(MAX_UCAN_CHAIN_DEPTH_DEFAULT)
  })
  it('returns default for unparseable value', () => {
    expect(parseMaxUcanChainDepth('abc')).toBe(MAX_UCAN_CHAIN_DEPTH_DEFAULT)
  })
  it('returns default for below-min value', () => {
    expect(parseMaxUcanChainDepth('0')).toBe(MAX_UCAN_CHAIN_DEPTH_DEFAULT)
  })
  it('returns default for above-max value', () => {
    expect(parseMaxUcanChainDepth(String(MAX_UCAN_CHAIN_DEPTH_MAX + 1))).toBe(
      MAX_UCAN_CHAIN_DEPTH_DEFAULT,
    )
    expect(parseMaxUcanChainDepth('999')).toBe(MAX_UCAN_CHAIN_DEPTH_DEFAULT)
  })
  it('returns stored value when within range', () => {
    expect(parseMaxUcanChainDepth('3')).toBe(3)
    expect(parseMaxUcanChainDepth(String(MAX_UCAN_CHAIN_DEPTH_MIN))).toBe(
      MAX_UCAN_CHAIN_DEPTH_MIN,
    )
    expect(parseMaxUcanChainDepth(String(MAX_UCAN_CHAIN_DEPTH_MAX))).toBe(
      MAX_UCAN_CHAIN_DEPTH_MAX,
    )
  })
})

describe('useSyncConfigStore — maxUcanChainDepth load/save', () => {
  it('loadConfigAsync keeps default when no row exists', async () => {
    // Three empty select responses (debounce, interval, depth).
    selectQueue = [[], [], []]
    const store = useSyncConfigStore()
    await store.loadConfigAsync()
    expect(store.config.maxUcanChainDepth).toBe(MAX_UCAN_CHAIN_DEPTH_DEFAULT)
  })

  it('loadConfigAsync uses stored value when in range', async () => {
    selectQueue = [[], [], [{ value: '3' }]]
    const store = useSyncConfigStore()
    await store.loadConfigAsync()
    expect(store.config.maxUcanChainDepth).toBe(3)
  })

  it('loadConfigAsync falls back to default when stored value is unparseable', async () => {
    selectQueue = [[], [], [{ value: 'abc' }]]
    const store = useSyncConfigStore()
    await store.loadConfigAsync()
    expect(store.config.maxUcanChainDepth).toBe(MAX_UCAN_CHAIN_DEPTH_DEFAULT)
  })

  it('loadConfigAsync falls back to default when stored value is out of range', async () => {
    selectQueue = [[], [], [{ value: '999' }]]
    const store = useSyncConfigStore()
    await store.loadConfigAsync()
    expect(store.config.maxUcanChainDepth).toBe(MAX_UCAN_CHAIN_DEPTH_DEFAULT)
  })

  it('saveConfigAsync inserts new value when no existing row', async () => {
    // upsertSettingAsync does a check-select first; return empty so it INSERTs.
    selectQueue = [[]]
    const store = useSyncConfigStore()
    await store.saveConfigAsync({ maxUcanChainDepth: 7 })
    expect(store.config.maxUcanChainDepth).toBe(7)
    expect(insertSpy).toHaveBeenCalledTimes(1)
    const inserted = insertSpy.mock.calls[0]?.[0] as unknown as {
      key: string
      value: string
    } | undefined
    expect(inserted?.key).toBe('max_ucan_chain_depth')
    expect(inserted?.value).toBe('7')
  })

  it('saveConfigAsync updates existing row when one is present', async () => {
    // upsertSettingAsync's check-select returns a row → UPDATE path.
    selectQueue = [[{ value: '5' }]]
    const store = useSyncConfigStore()
    await store.saveConfigAsync({ maxUcanChainDepth: 8 })
    expect(store.config.maxUcanChainDepth).toBe(8)
    expect(updateSpy).toHaveBeenCalledTimes(1)
    expect(insertSpy).not.toHaveBeenCalled()
  })

  it('saveConfigAsync rejects below-min value without touching db or reactive state', async () => {
    const store = useSyncConfigStore()
    const original = store.config.maxUcanChainDepth
    await expect(
      store.saveConfigAsync({ maxUcanChainDepth: 0 }),
    ).rejects.toThrow(/out of range/)
    expect(insertSpy).not.toHaveBeenCalled()
    expect(updateSpy).not.toHaveBeenCalled()
    expect(store.config.maxUcanChainDepth).toBe(original)
  })

  it('saveConfigAsync rejects above-max value without touching db or reactive state', async () => {
    const store = useSyncConfigStore()
    const original = store.config.maxUcanChainDepth
    await expect(
      store.saveConfigAsync({ maxUcanChainDepth: MAX_UCAN_CHAIN_DEPTH_MAX + 1 }),
    ).rejects.toThrow(/out of range/)
    expect(insertSpy).not.toHaveBeenCalled()
    expect(updateSpy).not.toHaveBeenCalled()
    expect(store.config.maxUcanChainDepth).toBe(original)
  })

  it('saveConfigAsync rejects non-integer values', async () => {
    const store = useSyncConfigStore()
    await expect(
      store.saveConfigAsync({ maxUcanChainDepth: 5.5 }),
    ).rejects.toThrow(/out of range/)
    expect(insertSpy).not.toHaveBeenCalled()
    expect(updateSpy).not.toHaveBeenCalled()
  })
})

describe('DEFAULT_SYNC_CONFIG', () => {
  it('exposes maxUcanChainDepth = 5', () => {
    expect(DEFAULT_SYNC_CONFIG.maxUcanChainDepth).toBe(5)
    expect(MAX_UCAN_CHAIN_DEPTH_DEFAULT).toBe(5)
    expect(MAX_UCAN_CHAIN_DEPTH_MIN).toBe(1)
    expect(MAX_UCAN_CHAIN_DEPTH_MAX).toBe(20)
  })
})
