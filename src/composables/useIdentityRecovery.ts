import {
  decryptPrivateKeyAsync,
  publicKeyToDidKeyAsync,
} from '@haex-space/vault-sdk'

export interface RecoveryKeyData {
  did: string
  publicKey: string
  encryptedPrivateKey: string
  privateKeyNonce: string
  privateKeySalt: string
}

/**
 * Composable for recovering an identity from a sync server via email + OTP.
 *
 * Flow:
 * 1. requestOtpAsync(serverUrl, email) -> triggers OTP email
 * 2. verifyOtpAsync(serverUrl, email, code) -> returns encrypted private key
 * 3. decryptAndImportAsync(recoveryData, vaultPassword) -> imports identity locally
 */
export const useIdentityRecovery = () => {
  const identityStore = useIdentityStore()

  const isLoading = ref(false)
  const error = ref<string | null>(null)

  /**
   * Request OTP code to be sent to the user's email.
   */
  const requestOtpAsync = async (
    serverUrl: string,
    email: string,
  ): Promise<boolean> => {
    isLoading.value = true
    error.value = null

    try {
      const res = await fetch(
        `${serverUrl}/identity-auth/recover-request`,
        {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ email }),
        },
      )

      if (!res.ok) {
        const data = await res.json().catch(() => ({ error: 'Unknown error' }))
        throw new Error(data.error || `Request failed: HTTP ${res.status}`)
      }

      return true
    } catch (err) {
      console.error('[RECOVERY] OTP request failed:', err)
      error.value = err instanceof Error ? err.message : 'Unknown error'
      return false
    } finally {
      isLoading.value = false
    }
  }

  /**
   * Verify OTP code and retrieve encrypted private key from server.
   */
  const verifyOtpAsync = async (
    serverUrl: string,
    email: string,
    code: string,
  ): Promise<RecoveryKeyData | null> => {
    isLoading.value = true
    error.value = null

    try {
      const res = await fetch(
        `${serverUrl}/identity-auth/recover-verify`,
        {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ email, code }),
        },
      )

      if (!res.ok) {
        const data = await res.json().catch(() => ({ error: 'Unknown error' }))
        throw new Error(data.error || `Verification failed: HTTP ${res.status}`)
      }

      return await res.json()
    } catch (err) {
      console.error('[RECOVERY] OTP verify failed:', err)
      error.value = err instanceof Error ? err.message : 'Unknown error'
      return null
    } finally {
      isLoading.value = false
    }
  }

  /**
   * Decrypt the recovered private key with the vault password and import as local identity.
   * Returns the local identity ID.
   */
  const decryptAndImportAsync = async (
    recoveryData: RecoveryKeyData,
    vaultPassword: string,
  ): Promise<string | null> => {
    isLoading.value = true
    error.value = null

    try {
      // Decrypt private key using vault password
      const privateKeyBase64 = await decryptPrivateKeyAsync(
        recoveryData.encryptedPrivateKey,
        recoveryData.privateKeyNonce,
        recoveryData.privateKeySalt,
        vaultPassword,
      )

      // Verify DID matches public key
      const derivedDid = await publicKeyToDidKeyAsync(recoveryData.publicKey)
      if (derivedDid !== recoveryData.did) {
        throw new Error('DID mismatch: recovered key does not match expected identity')
      }

      // Check if identity already exists locally
      const existingIdentities = identityStore.identities
      const existingIdentity = existingIdentities.find(
        (i) => i.did === recoveryData.did,
      )

      if (existingIdentity) {
        // Identity already exists locally, just return its ID
        return existingIdentity.id
      }

      // Import identity into local store
      const identity = await identityStore.importIdentityAsync({
        did: recoveryData.did,
        label: 'Recovered Identity',
        publicKey: recoveryData.publicKey,
        privateKey: privateKeyBase64,
      })

      return identity.id
    } catch (err) {
      console.error('[RECOVERY] Decrypt/import failed:', err)
      error.value = err instanceof Error ? err.message : 'Unknown error'
      return null
    } finally {
      isLoading.value = false
    }
  }

  /**
   * Resend OTP code.
   */
  const resendOtpAsync = async (
    serverUrl: string,
    email: string,
  ): Promise<boolean> => {
    return requestOtpAsync(serverUrl, email)
  }

  return {
    isLoading: readonly(isLoading),
    error: readonly(error),
    requestOtpAsync,
    verifyOtpAsync,
    decryptAndImportAsync,
    resendOtpAsync,
  }
}
