import { describe, it, expect, vi, beforeEach } from 'vitest'

// Mock Tauri fetch
vi.mock('@tauri-apps/plugin-http', () => ({
  fetch: vi.fn(),
}))

// Mock didAuth
vi.mock('@/utils/auth/didAuth', () => ({
  fetchWithDidAuth: vi.fn(),
}))

vi.mock('~/stores/vault', () => ({
  requireDb: vi.fn(),
}))

vi.mock('@haex-space/marketplace-sdk', () => ({
  createMarketplaceClient: vi.fn(),
}))

vi.mock('@/stores/logging', () => ({
  createLogger: () => ({
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  }),
}))

import { fetch as mockTauriFetch } from '@tauri-apps/plugin-http'
import { fetchWithDidAuth } from '@/utils/auth/didAuth'
import { buildAuthedFetch, ensureDefaultMarketplaceAsync, useMarketplaces } from '@/composables/useMarketplaces'
import { requireDb } from '~/stores/vault'
import { createMarketplaceClient } from '@haex-space/marketplace-sdk'

const mockRow = (overrides = {}) => ({
  id: 'test-id',
  name: 'Test',
  baseUrl: 'https://example.com',
  enabled: true,
  isDefault: false,
  sortOrder: 10,
  authType: 'none' as const,
  authToken: null,
  authUsername: null,
  authPassword: null,
  authIdentityId: null,
  createdAt: null,
  updatedAt: null,
  ...overrides,
})

describe('buildAuthedFetch', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(mockTauriFetch).mockResolvedValue(new Response('{}', { status: 200 }))
    vi.mocked(fetchWithDidAuth).mockResolvedValue(new Response('{}', { status: 200 }))
  })

  it('none: calls tauri fetch with no extra auth header', async () => {
    const fetcher = buildAuthedFetch(mockRow())
    await fetcher('https://example.com/extensions', { method: 'GET' })

    expect(mockTauriFetch).toHaveBeenCalledWith(
      'https://example.com/extensions',
      expect.objectContaining({ method: 'GET' }),
    )
    const callHeaders = (vi.mocked(mockTauriFetch).mock.calls[0]![1] as RequestInit | undefined)?.headers as Record<string, string> | undefined
    expect(callHeaders?.Authorization).toBeUndefined()
  })

  it('bearer: adds Authorization: Bearer header', async () => {
    const fetcher = buildAuthedFetch(mockRow({ authType: 'bearer', authToken: 'my-secret' }))
    await fetcher('https://example.com/extensions', {})

    const callHeaders = (vi.mocked(mockTauriFetch).mock.calls[0]![1] as RequestInit).headers as Record<string, string>
    expect(callHeaders.Authorization).toBe('Bearer my-secret')
  })

  it('basic: adds Authorization: Basic header', async () => {
    const fetcher = buildAuthedFetch(mockRow({ authType: 'basic', authUsername: 'user', authPassword: 'pass' }))
    await fetcher('https://example.com/extensions', {})

    const callHeaders = (vi.mocked(mockTauriFetch).mock.calls[0]![1] as RequestInit).headers as Record<string, string>
    expect(callHeaders.Authorization).toBe(`Basic ${btoa('user:pass')}`)
  })

  it('did: delegates to fetchWithDidAuth with marketplace:list action', async () => {
    const fetcher = buildAuthedFetch(
      mockRow({ authType: 'did', authIdentityId: 'identity-1' }),
      { did: 'did:key:test', privateKey: 'privkey-b64' },
    )
    await fetcher('https://example.com/extensions', { method: 'GET' })

    expect(fetchWithDidAuth).toHaveBeenCalledWith(
      'https://example.com/extensions',
      'privkey-b64',
      'did:key:test',
      'marketplace:list',
      expect.objectContaining({ method: 'GET' }),
    )
    expect(mockTauriFetch).not.toHaveBeenCalled()
  })
})

// ─── Helpers for useMarketplaces tests ───────────────────────────────────────

function setupDbMock(mockDb: { select: ReturnType<typeof vi.fn> }, rows: unknown[]) {
  const orderByMock = vi.fn().mockResolvedValue(rows)
  const whereMock = vi.fn().mockReturnValue({ orderBy: orderByMock })
  const fromMock = vi.fn().mockReturnValue({ where: whereMock })
  mockDb.select.mockReturnValue({ from: fromMock })
}

const makeExt = (extensionId: string, name: string) => ({
  id: extensionId,
  extensionId,
  name,
  slug: name.toLowerCase(),
  shortDescription: 'desc',
  iconUrl: null,
  verified: false,
  totalDownloads: 0,
  averageRating: null,
  reviewCount: 0,
  tags: null,
  publishedAt: null,
  publisher: null,
  category: null,
  versions: [],
})

const makeMarketplaceRow = (id: string, overrides: Record<string, unknown> = {}) => ({
  ...mockRow({ id, name: `Market ${id}`, baseUrl: `https://market-${id}.example`, isDefault: id === 'default' }),
  ...overrides,
})

// ─── Tests ───────────────────────────────────────────────────────────────────
describe('useMarketplaces', () => {
  let mockDb: { select: ReturnType<typeof vi.fn> }

  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(mockTauriFetch).mockResolvedValue(new Response('{}', { status: 200 }))
    mockDb = { select: vi.fn() }
    vi.mocked(requireDb).mockReturnValue(mockDb as unknown as ReturnType<typeof requireDb>)
  })

  it('merges extensions from two sources and tags sourceMarketplaceId', async () => {
    const rows = [makeMarketplaceRow('a'), makeMarketplaceRow('b')]
    setupDbMock(mockDb, rows)

    const clientA = { listExtensions: vi.fn().mockResolvedValue({ extensions: [makeExt('ext-1', 'Alpha')], pagination: { total: 1 } }) }
    const clientB = { listExtensions: vi.fn().mockResolvedValue({ extensions: [makeExt('ext-2', 'Beta')], pagination: { total: 1 } }) }
    vi.mocked(createMarketplaceClient)
      .mockReturnValueOnce(clientA as unknown as ReturnType<typeof createMarketplaceClient>)
      .mockReturnValueOnce(clientB as unknown as ReturnType<typeof createMarketplaceClient>)

    const { extensions, fetchExtensions } = useMarketplaces()
    await fetchExtensions()

    expect(extensions.value).toHaveLength(2)
    expect(extensions.value.find(e => e.extensionId === 'ext-1')?.sourceMarketplaceId).toBe('a')
    expect(extensions.value.find(e => e.extensionId === 'ext-2')?.sourceMarketplaceId).toBe('b')
  })

  it('dedupes by extensionId, keeps first (lowest sort_order)', async () => {
    const rows = [makeMarketplaceRow('a', { sortOrder: 1 }), makeMarketplaceRow('b', { sortOrder: 2 })]
    setupDbMock(mockDb, rows)

    const sharedExt = makeExt('shared-id', 'Shared')
    const clientA = { listExtensions: vi.fn().mockResolvedValue({ extensions: [sharedExt], pagination: { total: 1 } }) }
    const clientB = { listExtensions: vi.fn().mockResolvedValue({ extensions: [sharedExt], pagination: { total: 1 } }) }
    vi.mocked(createMarketplaceClient)
      .mockReturnValueOnce(clientA as unknown as ReturnType<typeof createMarketplaceClient>)
      .mockReturnValueOnce(clientB as unknown as ReturnType<typeof createMarketplaceClient>)

    const { extensions, fetchExtensions } = useMarketplaces()
    await fetchExtensions()

    expect(extensions.value).toHaveLength(1)
    expect(extensions.value[0]!.sourceMarketplaceId).toBe('a')
  })

  it('per-source failure leaves other results and records the error', async () => {
    const rows = [makeMarketplaceRow('ok'), makeMarketplaceRow('broken')]
    setupDbMock(mockDb, rows)

    const clientOk = { listExtensions: vi.fn().mockResolvedValue({ extensions: [makeExt('ext-ok', 'Fine')], pagination: { total: 1 } }) }
    const clientBroken = { listExtensions: vi.fn().mockRejectedValue(new Error('network error')) }
    vi.mocked(createMarketplaceClient)
      .mockReturnValueOnce(clientOk as unknown as ReturnType<typeof createMarketplaceClient>)
      .mockReturnValueOnce(clientBroken as unknown as ReturnType<typeof createMarketplaceClient>)

    const { extensions, sourceErrors, fetchExtensions } = useMarketplaces()
    await fetchExtensions()

    expect(extensions.value).toHaveLength(1)
    expect(extensions.value[0]!.extensionId).toBe('ext-ok')
    expect(sourceErrors.value['broken']).toEqual({ name: 'Market broken', message: 'network error' })
  })
})

describe('ensureDefaultMarketplaceAsync', () => {
  let mockDb: {
    select: ReturnType<typeof vi.fn>
    insert: ReturnType<typeof vi.fn>
  }

  beforeEach(() => {
    vi.clearAllMocks()
    mockDb = { select: vi.fn(), insert: vi.fn() }
    vi.mocked(requireDb).mockReturnValue(mockDb as unknown as ReturnType<typeof requireDb>)
  })

  const stubSelect = (rows: unknown[]) => {
    const limitMock = vi.fn().mockResolvedValue(rows)
    const whereMock = vi.fn().mockReturnValue({ limit: limitMock })
    const fromMock = vi.fn().mockReturnValue({ where: whereMock })
    mockDb.select.mockReturnValue({ from: fromMock })
  }

  it('inserts the default row when none exists', async () => {
    stubSelect([])
    const valuesMock = vi.fn().mockResolvedValue(undefined)
    mockDb.insert.mockReturnValue({ values: valuesMock })

    await ensureDefaultMarketplaceAsync()

    expect(valuesMock).toHaveBeenCalledWith(expect.objectContaining({
      name: 'Haex Marketplace',
      baseUrl: 'https://marketplace.haex.space',
      enabled: true,
      isDefault: true,
      sortOrder: 1,
      authType: 'none',
    }))
  })

  it('does not insert when a default row already exists', async () => {
    stubSelect([{ id: 'existing', isDefault: true }])

    await ensureDefaultMarketplaceAsync()

    expect(mockDb.insert).not.toHaveBeenCalled()
  })
})
