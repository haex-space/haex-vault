import {
  encryptPrivateKeyAsync,
} from '@haex-space/vault-sdk'
import { DidAuthAction } from '@haex-space/ucan'
import { fetchWithDidAuth } from '@/utils/auth/didAuth'
import { throwIfNotOk } from '~/utils/fetch'

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

  const { isLoading, error, execute } = useAsyncOperation({
    onError: (err) => console.error('[UPDATE PASSWORD]', err),
  })

  const updatePasswordAsync = (
    identityId: string,
    newPassword: string,
  ): Promise<boolean> =>
    execute(async () => {
      const identity = await identityStore.getIdentityByIdAsync(identityId)
      if (!identity?.privateKey) throw new Error('Identity not found or has no private key')

      await syncBackendsStore.loadBackendsAsync()

      const backends = syncBackendsStore.backends.filter(
        (b) => b.identityId === identityId,
      )

      if (backends.length === 0) return true

      const { encryptedPrivateKey, nonce, salt } =
        await encryptPrivateKeyAsync(identity.privateKey, newPassword)
      const privateKeyNonce = nonce
      const privateKeySalt = salt

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
          await throwIfNotOk(res, 'update recovery key')
        } catch (err) {
          console.error(`[UPDATE PASSWORD] Failed for backend ${backend.homeServerUrl}:`, err)
          failures.push(backend.homeServerUrl)
        }
      }

      if (failures.length > 0) {
        throw new Error(`Failed to update recovery key on: ${failures.join(', ')}`)
      }

      return true
    }).catch(() => false)

  return {
    isLoading,
    error,
    updatePasswordAsync,
  }
}
