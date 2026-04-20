import {
  importUserPrivateKeyAsync,
  encryptPrivateKeyAsync,
  didKeyToPublicKeyAsync,
} from '@haex-space/vault-sdk'
import { didAuthenticateAsync } from '~/stores/sync/engine/tokenManager'
import { throwIfNotOk, safeJson } from '~/utils/fetch'

export interface ServerRequirements {
  serverName: string
  claims: { type: string; required: boolean; label: string }[]
  didMethods: string[]
  serverTime?: string
}

export type CreateConnectionResult =
  | { status: 'connected'; backendId: string }
  | { status: 'verification_pending'; did: string; originUrl: string; identityId: string; approvedClaims: Record<string, string> }

interface SignedClaimPresentation {
  did: string
  publicKey: string
  claims: Record<string, string>
  timestamp: string
  signature: string
}

/**
 * Signs a claim presentation for selective disclosure.
 * Canonical form: did\0timestamp\0type1=value1\0type2=value2\0...
 */
async function signClaimPresentation(
  did: string,
  publicKeyBase64: string,
  claims: Record<string, string>,
  privateKeyBase64: string,
  clockOffsetMs: number = 0,
): Promise<SignedClaimPresentation> {
  const timestamp = new Date(Date.now() + clockOffsetMs).toISOString()
  const sortedEntries = Object.entries(claims).sort(([a], [b]) => a.localeCompare(b))
  const canonical = [did, timestamp, ...sortedEntries.map(([k, v]) => `${k}=${v}`)].join('\0')

  const privateKey = await importUserPrivateKeyAsync(privateKeyBase64)
  const data = new TextEncoder().encode(canonical)
  const sig = await crypto.subtle.sign(
    'Ed25519',
    privateKey,
    data,
  )

  return {
    did,
    publicKey: publicKeyBase64,
    claims,
    timestamp,
    signature: btoa(String.fromCharCode(...new Uint8Array(sig))),
  }
}

/**
 * Composable for creating a new sync connection to the current vault.
 * Uses identity-based (DID) challenge-response authentication.
 *
 * Flow:
 * 1. createConnectionAsync() → registers with server
 *    - If new user: returns { status: 'verification_pending', did }
 *    - If already verified: completes connection and returns { status: 'connected', backendId }
 * 2. verifyEmailAsync(originUrl, did, code) → verifies OTP code
 * 3. completeConnectionAsync(params) → logs in and starts sync
 */
export const useCreateSyncConnection = () => {
  const syncBackendsStore = useSyncBackendsStore()
  const syncEngineStore = useSyncEngineStore()
  const syncOrchestratorStore = useSyncOrchestratorStore()
  const vaultStore = useVaultStore()
  const identityStore = useIdentityStore()
  const { currentVaultId, currentVaultName, currentVaultPassword } =
    storeToRefs(vaultStore)

  const { isLoading, error, execute } = useAsyncOperation({
    onError: (err) => console.error('[SYNC]', err),
  })
  const serverClockOffsetMs = ref(0)

  const getBackendNameFromUrl = (url: string): string => {
    try {
      const hostname = new URL(url).hostname
      if (hostname === 'sync.haex.space') {
        return 'HaexSpace Sync'
      }
      return hostname
    } catch {
      return 'Sync Server'
    }
  }

  const fetchRequirementsAsync = async (originUrl: string): Promise<ServerRequirements> => {
    const requestedAt = Date.now()
    const res = await fetch(`${originUrl}/identity-auth/requirements`)
    await throwIfNotOk(res, 'fetch requirements')
    const data: ServerRequirements = await res.json()

    if (data.serverTime) {
      const serverTimeMs = new Date(data.serverTime).getTime()
      const roundTripMs = Date.now() - requestedAt
      const estimatedServerNow = serverTimeMs + roundTripMs / 2
      serverClockOffsetMs.value = estimatedServerNow - Date.now()

      if (Math.abs(serverClockOffsetMs.value) > 1000) {
        console.warn(`[SYNC] Clock skew detected: ${serverClockOffsetMs.value}ms (device is ${serverClockOffsetMs.value > 0 ? 'behind' : 'ahead'})`)
      }
    }

    return data
  }

  const loginAsync = async (originUrl: string, identityId: string): Promise<{ access_token: string; refresh_token: string }> => {
    const identity = await identityStore.getIdentityByIdAsync(identityId)
    if (!identity?.privateKey) {
      throw new Error('Identity not found or has no private key')
    }

    return didAuthenticateAsync(originUrl, identity.did, identity.privateKey)
  }

  /**
   * Registers with the server. If the identity is already verified,
   * completes the full connection. Otherwise returns verification_pending.
   */
  const createConnectionAsync = (params: {
    originUrl: string
    identityId: string
    approvedClaims: Record<string, string>
  }): Promise<CreateConnectionResult | null> =>
    execute(async (): Promise<CreateConnectionResult | null> => {
      const { backends } = storeToRefs(syncBackendsStore)

      const existingBackend = backends.value.find(
        (b) => b.homeServerUrl === params.originUrl,
      )
      if (existingBackend) {
        throw new Error(`A connection to ${params.originUrl} already exists`)
      }

      const identity = await identityStore.getIdentityByIdAsync(params.identityId)
      if (!identity?.privateKey) {
        throw new Error('Identity not found or has no private key')
      }

      const presentation = await signClaimPresentation(
        identity.did,
        await didKeyToPublicKeyAsync(identity.did),
        params.approvedClaims,
        identity.privateKey,
        serverClockOffsetMs.value,
      )

      const registrationBody: Record<string, unknown> = { presentation }

      const identityPassword = identityStore.consumeIdentityPassword(params.identityId) ?? currentVaultPassword.value
      if (identityPassword) {
        try {
          const recovery = await encryptPrivateKeyAsync(
            identity.privateKey,
            identityPassword,
          )
          registrationBody.encryptedPrivateKey = recovery.encryptedPrivateKey
          registrationBody.privateKeyNonce = recovery.nonce
          registrationBody.privateKeySalt = recovery.salt
        } catch (e) {
          console.error('[SYNC] Failed to encrypt private key for recovery — account recovery will not be available:', e)
        }
      } else {
        console.warn('[SYNC] No vault password available — recovery key will not be uploaded. Account recovery from another device will not be possible.')
      }

      const registerRes = await fetch(`${params.originUrl}/identity-auth/register`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(registrationBody),
      })

      const registerData = await safeJson<{ status?: string; did?: string; error?: string }>(registerRes)

      if (!registerRes.ok) {
        if (registerRes.status !== 409 || registerData.error?.includes('another identity')) {
          throw new Error(`Registration failed: ${registerData.error || 'Unknown error'}`)
        }
        // 409 with 'DID already registered' = already registered and verified — proceed to login
      } else if (registerData.status === 'verification_pending') {
        return {
          status: 'verification_pending' as const,
          did: registerData.did ?? identity.did,
          originUrl: params.originUrl,
          identityId: params.identityId,
          approvedClaims: params.approvedClaims,
        }
      }

      const backendId = await setupBackendAsync(params.originUrl, params.identityId)
      if (!backendId) return null

      return { status: 'connected' as const, backendId }
    }).catch(() => null)

  /**
   * Verifies the email OTP code with the server.
   */
  const verifyEmailAsync = (originUrl: string, did: string, code: string): Promise<boolean> =>
    execute(async () => {
      const res = await fetch(`${originUrl}/identity-auth/verify-email`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ did, code }),
      })
      await throwIfNotOk(res, 'verify email')
      return true
    }).catch(() => false)

  /**
   * Resends the verification code.
   */
  const resendVerificationAsync = (originUrl: string, did: string): Promise<boolean> =>
    execute(async () => {
      const res = await fetch(`${originUrl}/identity-auth/resend-verification`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ did }),
      })
      await throwIfNotOk(res, 'resend verification code')
      return true
    }).catch(() => false)

  /**
   * Completes the connection after email verification:
   * login via challenge-response, create backend, ensure sync key, start sync.
   */
  const completeConnectionAsync = (params: {
    originUrl: string
    identityId: string
  }): Promise<string | null> =>
    execute(() => setupBackendAsync(params.originUrl, params.identityId))
      .catch(() => null)

  /**
   * Internal: creates backend entry, logs in, sets up sync key, starts sync.
   */
  const setupBackendAsync = async (originUrl: string, identityId: string): Promise<string | null> => {
    // Check if a backend with this URL already exists (e.g., reconnecting after data deletion)
    const existingBackend = await syncBackendsStore.findBackendByServerUrlAsync(originUrl)

    let backendId: string
    let createdNew = false

    if (existingBackend) {
      backendId = existingBackend.id
      // Ensure backends are loaded in memory (may be empty after a reset)
      await syncBackendsStore.loadBackendsAsync()
      await syncBackendsStore.updateBackendAsync(backendId, {
        enabled: false,
        identityId,
      })
    } else {
      const backendName = getBackendNameFromUrl(originUrl)
      const tempBackend = await syncBackendsStore.addBackendAsync({
        name: backendName,
        homeServerUrl: originUrl,
        enabled: false,
        spaceId: currentVaultId.value,
        identityId,
      })

      if (!tempBackend) {
        throw new Error('Failed to create backend entry')
      }

      backendId = tempBackend.id
      createdNew = true
    }

    try {
      syncEngineStore.initTokenManagerAsync(backendId)

      const session = await loginAsync(originUrl, identityId)
      syncEngineStore.setSession(backendId, session)

      if (!currentVaultPassword.value) {
        throw new Error('Vault password not available')
      }

      await syncEngineStore.ensureSyncKeyAsync(
        backendId,
        currentVaultId.value!,
        currentVaultName.value,
        currentVaultPassword.value,
        originUrl,
      )

      await syncBackendsStore.updateBackendAsync(backendId, {
        enabled: true,
      })

      await syncBackendsStore.loadBackendsAsync()
      await syncOrchestratorStore.startSyncAsync()

      return backendId
    } catch (err) {
      if (createdNew) {
        console.error('[SYNC] Connection setup failed, removing backend entry')
        await syncBackendsStore.deleteBackendAsync(backendId)
      } else {
        console.error('[SYNC] Connection setup failed for existing backend')
      }
      throw err
    }
  }

  return {
    isLoading,
    error,
    createConnectionAsync,
    verifyEmailAsync,
    resendVerificationAsync,
    completeConnectionAsync,
    fetchRequirementsAsync,
    loginAsync,
    getBackendNameFromUrl,
  }
}
