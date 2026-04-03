import {
  encryptPrivateKeyAsync,
} from '@haex-space/vault-sdk'
import { DidAuthAction } from '@haex-space/ucan'
import { fetchWithDidAuth } from '@/utils/auth/didAuth'
import { getErrorMessage } from '~/utils/errors'

/**
 * Composable for updating the identity password on all connected sync backends.
 *
 * Flow:
 * 1. Find all backends linked to this identity
 * 2. For each backend: challenge-response login → re-encrypt private key → POST /identity-auth/update-recovery
 */
export const useUpdateIdentityPassword = () => {
  const identityStore = useIdentityStore()
  const syncBackendsStore = useSyncBackendsStore()

  const isLoading = ref(false)
  const error = ref<string | null>(null)

  const updatePasswordAsync = async (
    identityId: string,
    newPassword: string,
  ): Promise<boolean> => {
    isLoading.value = true
    error.value = null

    try {
      const identity = await identityStore.getIdentityByIdAsync(identityId)
      if (!identity?.privateKey) throw new Error('Identity not found or has no private key')

      // Ensure backends are loaded
      await syncBackendsStore.loadBackendsAsync()

      // Find all backends connected to this identity
      const backends = syncBackendsStore.backends.filter(
        (b) => b.identityId === identityId,
      )

      if (backends.length === 0) {
        // No backends connected — nothing to update on the server
        return true
      }

      // Re-encrypt the private key with the new password once
      const { encryptedPrivateKey, nonce, salt } =
        await encryptPrivateKeyAsync(identity.privateKey, newPassword)
      const privateKeyNonce = nonce
      const privateKeySalt = salt

      // Update each connected backend
      const failures: string[] = []
      for (const backend of backends) {
        try {
          const res = await fetchWithDidAuth(
            `${backend.homeServerUrl}/identity-auth/update-recovery`,
            identity.privateKey,
            identity.did,
            DidAuthAction.UpdateRecovery,
            {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({ encryptedPrivateKey, privateKeyNonce, privateKeySalt }),
            },
          )

          if (!res.ok) {
            const data = await res.json().catch(() => ({ error: 'Unknown error' }))
            throw new Error(data.error || `HTTP ${res.status}`)
          }
        } catch (err) {
          console.error(`[UPDATE PASSWORD] Failed for backend ${backend.homeServerUrl}:`, err)
          failures.push(backend.homeServerUrl)
        }
      }

      if (failures.length > 0) {
        throw new Error(`Failed to update recovery key on: ${failures.join(', ')}`)
      }

      return true
    } catch (err) {
      console.error('[UPDATE PASSWORD] Failed:', err)
      error.value = getErrorMessage(err)
      return false
    } finally {
      isLoading.value = false
    }
  }

  return {
    isLoading: readonly(isLoading),
    error: readonly(error),
    updatePasswordAsync,
  }
}
