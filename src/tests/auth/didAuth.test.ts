import { describe, expect, it, vi } from 'vitest'
import { fetchWithDidAuth } from '@/utils/auth/didAuth'

vi.mock('@tauri-apps/plugin-http', () => ({
  fetch: vi.fn(),
}))

describe('DID-authenticated fetchers', () => {
  it('rejects non-string request bodies before signing', async () => {
    await expect(fetchWithDidAuth('https://example.test/resource', 'private-key', 'did:key:test', {
      method: 'POST',
      body: new URLSearchParams({ value: 'test' }),
    })).rejects.toThrow(/require string bodies/i)
  })
})
