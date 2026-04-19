import {
  decryptPrivateKeyAsync,
  publicKeyToDidKeyAsync,
} from '@haex-space/vault-sdk'
import { throwIfNotOk } from '~/utils/fetch'

export interface RecoveryKeyData {
  did: string
  publicKey: string
  encryptedPrivateKey: string
  privateKeyNonce: string
  privateKeySalt: string
  session?: {
    access_token: string
    refresh_token: string
    expires_in: number
    expires_at: number
  }
  identity?: {
    publicKey: string
    did: string
    tier: string
  }
}

/**
 * Composable for recovering an identity from a sync server via email + OTP.
 *
 * Flow:
 * 1. requestOtpAsync(originUrl, email) -> triggers OTP email
 * 2. verifyOtpAsync(originUrl, email, code) -> returns encrypted private key
 * 3. decryptAndImportAsync(recoveryData, vaultPassword) -> imports identity locally
 */
export const useIdentityRecovery = () => {
  const { isLoading, error, execute } = useAsyncOperation({
    onError: (err) => console.error('[RECOVERY]', err),
  })

  const requestOtpAsync = (
    originUrl: string,
    email: string,
  ): Promise<boolean> =>
    execute(async () => {
      const res = await fetch(`${originUrl}/identity-auth/recover-request`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email }),
      })
      await throwIfNotOk(res, 'request recovery OTP')
      return true
    }).catch(() => false)

  const verifyOtpAsync = (
    originUrl: string,
    email: string,
    code: string,
  ): Promise<RecoveryKeyData | null> =>
    execute(async () => {
      const res = await fetch(`${originUrl}/identity-auth/recover-verify`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email, code }),
      })
      await throwIfNotOk(res, 'verify recovery OTP')
      return (await res.json()) as RecoveryKeyData
    }).catch(() => null)

  /**
   * Decrypt the recovered private key with the vault password to verify it works.
   * Does NOT import into local vault (vault isn't open yet at this stage).
   */
  const decryptAndVerifyAsync = (
    recoveryData: RecoveryKeyData,
    vaultPassword: string,
  ): Promise<boolean> =>
    execute(async () => {
      await decryptPrivateKeyAsync(
        recoveryData.encryptedPrivateKey,
        recoveryData.privateKeyNonce,
        recoveryData.privateKeySalt,
        vaultPassword,
      )

      const derivedDid = await publicKeyToDidKeyAsync(recoveryData.publicKey)
      if (derivedDid !== recoveryData.did) {
        throw new Error('DID mismatch: recovered key does not match expected identity')
      }

      return true
    }).catch(() => false)

  const resendOtpAsync = (originUrl: string, email: string) =>
    requestOtpAsync(originUrl, email)

  return {
    isLoading,
    error,
    requestOtpAsync,
    verifyOtpAsync,
    decryptAndVerifyAsync,
    resendOtpAsync,
  }
}
