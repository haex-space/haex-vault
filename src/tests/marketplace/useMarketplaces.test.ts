import { describe, it, expect, vi, beforeEach } from 'vitest'

// Mock Tauri fetch
vi.mock('@tauri-apps/plugin-http', () => ({
  fetch: vi.fn(),
}))

// Mock didAuth
vi.mock('@/utils/auth/didAuth', () => ({
  fetchWithDidAuth: vi.fn(),
}))

import { fetch as mockTauriFetch } from '@tauri-apps/plugin-http'
import { fetchWithDidAuth } from '@/utils/auth/didAuth'
import { buildAuthedFetch } from '@/composables/useMarketplaces'

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
    const callHeaders = vi.mocked(mockTauriFetch).mock.calls[0][1]?.headers as Record<string, string> | undefined
    expect(callHeaders?.Authorization).toBeUndefined()
  })

  it('bearer: adds Authorization: Bearer header', async () => {
    const fetcher = buildAuthedFetch(mockRow({ authType: 'bearer', authToken: 'my-secret' }))
    await fetcher('https://example.com/extensions', {})

    const callHeaders = vi.mocked(mockTauriFetch).mock.calls[0][1]?.headers as Record<string, string>
    expect(callHeaders.Authorization).toBe('Bearer my-secret')
  })

  it('basic: adds Authorization: Basic header', async () => {
    const fetcher = buildAuthedFetch(mockRow({ authType: 'basic', authUsername: 'user', authPassword: 'pass' }))
    await fetcher('https://example.com/extensions', {})

    const callHeaders = vi.mocked(mockTauriFetch).mock.calls[0][1]?.headers as Record<string, string>
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
