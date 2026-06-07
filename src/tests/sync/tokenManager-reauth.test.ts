/**
 * D5 regression guard — concurrent DID re-authentication must dedupe.
 *
 * Behavioral: two parallel callers that detect an expired token at the
 * same time must share a single in-flight re-auth — exactly one
 * /identity-auth/challenge round trip, no overlapping setSession races.
 *
 * Structural: the manager keeps a single in-flight slot, not two parallel
 * ones (pendingReauthPromise + pendingReauth previously did the same job
 * at two layers, with the inner one already serving the outer's purpose).
 */
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}))

vi.mock('@tauri-apps/plugin-http', () => ({
  fetch: vi.fn(),
}))

vi.mock('@haex-space/vault-sdk', () => ({
  importUserPrivateKeyAsync: vi.fn().mockResolvedValue({ /* CryptoKey stub */ }),
}))

vi.mock('@/stores/sync/engine/types', () => ({
  engineLog: {
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  },
}))

import { fetch as mockFetch } from '@tauri-apps/plugin-http'
import {
  initTokenManager,
  setSession,
  setReauthResolver,
  getAuthTokenAsync,
  fetchWithReauthAsync,
  clearTokenState,
} from '@/stores/sync/engine/tokenManager'

const BACKEND_ID = 'tm-reauth-test'
const HOME_SERVER = 'https://sync.example'

const makeJwt = (expSecondsFromNow: number): string => {
  const payload = { exp: Math.floor(Date.now() / 1000) + expSecondsFromNow }
  const header = btoa(JSON.stringify({ alg: 'EdDSA' }))
  const body = btoa(JSON.stringify(payload))
  return `${header}.${body}.sig`
}

const installCryptoSubtleSignStub = (): void => {
  if (!globalThis.crypto?.subtle) {
    Object.defineProperty(globalThis, 'crypto', {
      configurable: true,
      value: {
        subtle: {
          sign: vi.fn().mockResolvedValue(new ArrayBuffer(64)),
        },
      },
    })
    return
  }
  vi.spyOn(globalThis.crypto.subtle, 'sign').mockResolvedValue(new ArrayBuffer(64))
}

describe('tokenManager — concurrent DID re-auth dedup (D5)', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    clearTokenState(BACKEND_ID)
    installCryptoSubtleSignStub()
  })

  afterEach(() => {
    clearTokenState(BACKEND_ID)
  })

  it('two parallel getAuthTokenAsync calls with an expired token trigger reauth exactly once', async () => {
    initTokenManager(BACKEND_ID)
    setReauthResolver(BACKEND_ID, async () => ({
      homeServerUrl: HOME_SERVER,
      did: 'did:key:zTest',
      privateKey: 'AAAA',
    }))
    setSession(BACKEND_ID, { access_token: makeJwt(-3600), refresh_token: 'r' })

    const freshToken = makeJwt(3600)
    let releaseChallenge: () => void = () => {}
    const challengeGate = new Promise<void>((r) => { releaseChallenge = r })

    ;(mockFetch as unknown as ReturnType<typeof vi.fn>).mockImplementation(async (url: string) => {
      if (url.endsWith('/identity-auth/challenge')) {
        await challengeGate
        return {
          ok: true,
          json: async () => ({ nonce: 'n' }),
        } as unknown as Response
      }
      if (url.endsWith('/identity-auth/verify')) {
        return {
          ok: true,
          json: async () => ({ access_token: freshToken, refresh_token: 'r2' }),
        } as unknown as Response
      }
      throw new Error(`unexpected fetch: ${url}`)
    })

    const callA = getAuthTokenAsync(BACKEND_ID)
    const callB = getAuthTokenAsync(BACKEND_ID)

    await Promise.resolve()
    await Promise.resolve()
    releaseChallenge()

    const [a, b] = await Promise.all([callA, callB])

    expect(a).toBe(freshToken)
    expect(b).toBe(freshToken)

    const fetchCalls = (mockFetch as unknown as ReturnType<typeof vi.fn>).mock.calls
    const challengeCalls = fetchCalls.filter((c) => String(c[0]).endsWith('/identity-auth/challenge'))
    const verifyCalls = fetchCalls.filter((c) => String(c[0]).endsWith('/identity-auth/verify'))

    expect(challengeCalls.length).toBe(1)
    expect(verifyCalls.length).toBe(1)
  })

  it('getAuthTokenAsync + fetchWithReauthAsync racing 401-retry share one reauth', async () => {
    initTokenManager(BACKEND_ID)
    setReauthResolver(BACKEND_ID, async () => ({
      homeServerUrl: HOME_SERVER,
      did: 'did:key:zTest',
      privateKey: 'AAAA',
    }))
    setSession(BACKEND_ID, { access_token: makeJwt(-3600), refresh_token: 'r' })

    const freshToken = makeJwt(3600)
    let releaseChallenge: () => void = () => {}
    const challengeGate = new Promise<void>((r) => { releaseChallenge = r })

    let protectedCalls = 0
    ;(mockFetch as unknown as ReturnType<typeof vi.fn>).mockImplementation(async (url: string) => {
      if (url.endsWith('/identity-auth/challenge')) {
        await challengeGate
        return {
          ok: true,
          json: async () => ({ nonce: 'n' }),
        } as unknown as Response
      }
      if (url.endsWith('/identity-auth/verify')) {
        return {
          ok: true,
          json: async () => ({ access_token: freshToken, refresh_token: 'r2' }),
        } as unknown as Response
      }
      if (url.endsWith('/protected')) {
        protectedCalls += 1
        return { status: protectedCalls === 1 ? 401 : 200 } as unknown as Response
      }
      throw new Error(`unexpected fetch: ${url}`)
    })

    const tokenCall = getAuthTokenAsync(BACKEND_ID)
    const protectedCall = fetchWithReauthAsync(
      `${HOME_SERVER}/protected`,
      { method: 'GET' },
      BACKEND_ID,
    )

    await Promise.resolve()
    await Promise.resolve()
    releaseChallenge()

    const [tokenResult, protectedResult] = await Promise.all([tokenCall, protectedCall])

    expect(tokenResult).toBe(freshToken)
    expect(protectedResult.status).toBe(200)

    const fetchCalls = (mockFetch as unknown as ReturnType<typeof vi.fn>).mock.calls
    const challengeCalls = fetchCalls.filter((c) => String(c[0]).endsWith('/identity-auth/challenge'))
    const verifyCalls = fetchCalls.filter((c) => String(c[0]).endsWith('/identity-auth/verify'))

    expect(challengeCalls.length).toBe(1)
    expect(verifyCalls.length).toBe(1)
  })
})

describe('tokenManager — single in-flight reauth slot (D5 structural guard)', () => {
  const source = readFileSync(
    resolve(__dirname, '../../stores/sync/engine/tokenManager.ts'),
    'utf-8',
  )

  it('exposes exactly one in-flight reauth field on TokenState', () => {
    const stateBlock = source.match(/interface TokenState \{[\s\S]*?\n\}/)
    expect(stateBlock).not.toBeNull()
    const block = stateBlock![0]
    const inflightFields = block.match(/^\s*\w*[Rr]eauth\w*: Promise/gm) ?? []
    expect(inflightFields.length).toBe(1)
  })

  it('no longer references the redundant pendingReauth field', () => {
    expect(source).not.toMatch(/\bpendingReauth\b(?!Promise)/)
  })
})
