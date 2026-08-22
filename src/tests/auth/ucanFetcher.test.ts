import { describe, it, expect, vi, beforeEach } from 'vitest'

// The identity store is a pinia store that reaches back through Vault stores,
// Tauri APIs, and the reactive DB. For a targeted unit test on resolvePrivateKey
// we stub only what it consumes: `identities` (a plain array field for the test).
const identitiesFixture: Array<{ did: string; privateKey: string | null }> = []

vi.mock('@/stores/identity', () => ({
  useIdentityStore: () => ({
    // Vue-reactive `.value` is unwrapped by the auto-imports plugin in the app,
    // but the fetcher reads `identityStore.identities.find(...)` — expose a
    // plain array here so `.find` behaves the same in the mocked path.
    identities: identitiesFixture,
  }),
}))

// `importUserPrivateKeyAsync` normally decodes base64 + imports as WebCrypto
// Ed25519. In tests we mock it to a lightweight stub that returns a
// distinguishable value per input so we can assert cache-hit reuse and
// per-DID invalidation without an actual WebCrypto surface.
const importSpy = vi.fn<(pk: string) => Promise<CryptoKey>>()

vi.mock('@haex-space/vault-sdk', () => ({
  importUserPrivateKeyAsync: (pk: string) => importSpy(pk),
}))

// Nuxt auto-imports (used e.g. by `useIdentityStore`) are not wired under
// vitest — but our mock above bypasses the auto-import path entirely, so
// nothing else must resolve.

import {
  resolvePrivateKey,
  clearIdentityKeyCache,
  invalidateIdentityKey,
} from '@/utils/auth/ucanFetcher'

const OWN_DID = 'did:key:zOwn123'
const OWN_PRIVATE_KEY = 'BASE64_OWN'
const UNKNOWN_DID = 'did:key:zUnknown999'
const CONTACT_DID = 'did:key:zContactAAA'

function seed(identities: Array<{ did: string; privateKey: string | null }>) {
  identitiesFixture.length = 0
  for (const id of identities) identitiesFixture.push(id)
}

beforeEach(() => {
  seed([])
  importSpy.mockReset()
  // Return a fresh sentinel per call so cache-hit tests can distinguish.
  importSpy.mockImplementation(async (pk) => ({ pk } as unknown as CryptoKey))
  clearIdentityKeyCache()
})

describe('resolvePrivateKey', () => {
  it('resolves an owned DID to a WebCrypto key', async () => {
    seed([{ did: OWN_DID, privateKey: OWN_PRIVATE_KEY }])

    const key = await resolvePrivateKey(OWN_DID)

    expect(key).toBeDefined()
    expect(importSpy).toHaveBeenCalledTimes(1)
    expect(importSpy).toHaveBeenCalledWith(OWN_PRIVATE_KEY)
  })

  it('throws for an unknown DID', async () => {
    seed([{ did: OWN_DID, privateKey: OWN_PRIVATE_KEY }])

    await expect(resolvePrivateKey(UNKNOWN_DID)).rejects.toThrow(
      /no unlocked own-identity/i,
    )
    expect(importSpy).not.toHaveBeenCalled()
  })

  it('throws for a contact (no privateKey)', async () => {
    // Contacts are represented by identities with a null private key. They
    // must not resolve — the vault owns no key for them.
    seed([
      { did: OWN_DID, privateKey: OWN_PRIVATE_KEY },
      { did: CONTACT_DID, privateKey: null },
    ])

    await expect(resolvePrivateKey(CONTACT_DID)).rejects.toThrow(
      /no unlocked own-identity/i,
    )
  })

  it('caches the imported key across repeated calls for the same DID', async () => {
    seed([{ did: OWN_DID, privateKey: OWN_PRIVATE_KEY }])

    const first = await resolvePrivateKey(OWN_DID)
    const second = await resolvePrivateKey(OWN_DID)

    expect(first).toBe(second)
    expect(importSpy).toHaveBeenCalledTimes(1)
  })

  it('clearIdentityKeyCache forces a re-import', async () => {
    seed([{ did: OWN_DID, privateKey: OWN_PRIVATE_KEY }])

    await resolvePrivateKey(OWN_DID)
    clearIdentityKeyCache()
    await resolvePrivateKey(OWN_DID)

    expect(importSpy).toHaveBeenCalledTimes(2)
  })

  it('invalidateIdentityKey drops one entry without affecting others', async () => {
    const OTHER_DID = 'did:key:zOther'
    seed([
      { did: OWN_DID, privateKey: OWN_PRIVATE_KEY },
      { did: OTHER_DID, privateKey: 'BASE64_OTHER' },
    ])

    await resolvePrivateKey(OWN_DID)
    await resolvePrivateKey(OTHER_DID)
    expect(importSpy).toHaveBeenCalledTimes(2)

    invalidateIdentityKey(OWN_DID)
    await resolvePrivateKey(OWN_DID)  // re-imports
    await resolvePrivateKey(OTHER_DID) // still cached

    expect(importSpy).toHaveBeenCalledTimes(3)
  })
})
