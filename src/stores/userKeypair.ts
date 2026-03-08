import {
  generateUserKeypairAsync,
  exportUserKeypairAsync,
  encryptPrivateKeyAsync,
  decryptPrivateKeyAsync,
} from '@haex-space/vault-sdk'
import { createLogger } from '@/stores/logging'
import { getAuthTokenAsync } from '@/stores/sync/engine/supabase'

const log = createLogger('USER KEYPAIR')

export const useUserKeypairStore = defineStore('userKeypairStore', () => {
  // Cached decrypted private key (Base64 PKCS8) - in memory only, never persisted
  const privateKeyBase64 = ref<string | null>(null)
  const publicKeyBase64 = ref<string | null>(null)
  const isRegistered = ref(false)

  /**
   * Ensures the user has a registered keypair.
   * If not registered, generates one, encrypts the private key, and registers on server.
   * If already registered, loads from server and decrypts.
   *
   * @param serverUrl - The sync server URL
   * @param password - The vault's server password (for encrypting/decrypting the private key)
   */
  const ensureKeypairAsync = async (serverUrl: string, password: string) => {
    // Try loading from server first
    const token = await getAuthTokenAsync()
    if (!token) throw new Error('Not authenticated')

    const response = await fetch(`${serverUrl}/keypairs/me`, {
      headers: { Authorization: `Bearer ${token}` },
    })

    if (response.ok) {
      // Keypair exists on server - load and decrypt
      const data = await response.json()
      publicKeyBase64.value = data.publicKey

      const decryptedPrivateKey = await decryptPrivateKeyAsync(
        data.encryptedPrivateKey,
        data.privateKeyNonce,
        data.privateKeySalt,
        password,
      )
      privateKeyBase64.value = decryptedPrivateKey
      isRegistered.value = true
      log.info('Loaded existing keypair from server')
      return
    }

    if (response.status !== 404) {
      throw new Error(`Failed to fetch keypair: ${response.statusText}`)
    }

    // No keypair yet - generate and register
    log.info('No keypair found, generating new one...')
    const keypair = await generateUserKeypairAsync()
    const exported = await exportUserKeypairAsync(keypair)

    // Encrypt private key with server password
    const encrypted = await encryptPrivateKeyAsync(exported.privateKey, password)

    // Register on server
    const registerResponse = await fetch(`${serverUrl}/keypairs`, {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${token}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        publicKey: exported.publicKey,
        encryptedPrivateKey: encrypted.encryptedPrivateKey,
        privateKeyNonce: encrypted.nonce,
        privateKeySalt: encrypted.salt,
      }),
    })

    if (!registerResponse.ok) {
      const error = await registerResponse.json().catch(() => ({}))
      throw new Error(`Failed to register keypair: ${error.error || registerResponse.statusText}`)
    }

    publicKeyBase64.value = exported.publicKey
    privateKeyBase64.value = exported.privateKey
    isRegistered.value = true
    log.info('Generated and registered new keypair')
  }

  const clearCache = () => {
    privateKeyBase64.value = null
    publicKeyBase64.value = null
    isRegistered.value = false
  }

  return {
    privateKeyBase64,
    publicKeyBase64,
    isRegistered,
    ensureKeypairAsync,
    clearCache,
  }
})
