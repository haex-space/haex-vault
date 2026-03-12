import {
  importUserPrivateKeyAsync,
  encryptPrivateKeyAsync,
} from '@haex-space/vault-sdk'

export interface ServerRequirements {
  serverName: string
  claims: { type: string; required: boolean; label: string }[]
  didMethods: string[]
}

export type CreateConnectionResult =
  | { status: 'connected'; backendId: string }
  | { status: 'verification_pending'; did: string; serverUrl: string; identityId: string; approvedClaims: Record<string, string> }

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
): Promise<SignedClaimPresentation> {
  const timestamp = new Date().toISOString()
  const sortedEntries = Object.entries(claims).sort(([a], [b]) => a.localeCompare(b))
  const canonical = [did, timestamp, ...sortedEntries.map(([k, v]) => `${k}=${v}`)].join('\0')

  const privateKey = await importUserPrivateKeyAsync(privateKeyBase64)
  const data = new TextEncoder().encode(canonical)
  const sig = await crypto.subtle.sign(
    { name: 'ECDSA', hash: 'SHA-256' },
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
 * 2. verifyEmailAsync(serverUrl, did, code) → verifies OTP code
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

  const isLoading = ref(false)
  const error = ref<string | null>(null)

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

  const fetchRequirementsAsync = async (serverUrl: string): Promise<ServerRequirements> => {
    const res = await fetch(`${serverUrl}/identity-auth/requirements`)
    if (!res.ok) {
      const data = await res.json().catch(() => ({ error: 'Unknown error' }))
      throw new Error(`Failed to fetch requirements: ${data.error || res.statusText}`)
    }
    return res.json()
  }

  const loginAsync = async (serverUrl: string, identityId: string): Promise<{ access_token: string; refresh_token: string }> => {
    const identity = await identityStore.getIdentityAsync(identityId)
    if (!identity) {
      throw new Error('Identity not found')
    }

    const challengeRes = await fetch(`${serverUrl}/identity-auth/challenge`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ did: identity.did }),
    })

    if (!challengeRes.ok) {
      const errorData = await challengeRes.json().catch(() => ({ error: 'Unknown error' }))
      throw new Error(`Challenge failed: ${errorData.error || 'Unknown error'}`)
    }

    const { nonce } = await challengeRes.json()

    const privateKey = await importUserPrivateKeyAsync(identity.privateKey)
    const sig = await crypto.subtle.sign(
      { name: 'ECDSA', hash: 'SHA-256' },
      privateKey,
      new TextEncoder().encode(nonce),
    )
    const signature = btoa(String.fromCharCode(...new Uint8Array(sig)))

    const verifyRes = await fetch(`${serverUrl}/identity-auth/verify`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ did: identity.did, nonce, signature }),
    })

    if (!verifyRes.ok) {
      const errorData = await verifyRes.json().catch(() => ({ error: 'Unknown error' }))
      throw new Error(`Verification failed: ${errorData.error || 'Unknown error'}`)
    }

    return verifyRes.json()
  }

  /**
   * Registers with the server. If the identity is already verified,
   * completes the full connection. Otherwise returns verification_pending.
   */
  const createConnectionAsync = async (params: {
    serverUrl: string
    identityId: string
    approvedClaims: Record<string, string>
  }): Promise<CreateConnectionResult | null> => {
    isLoading.value = true
    error.value = null

    try {
      const { backends } = storeToRefs(syncBackendsStore)

      const existingBackend = backends.value.find(
        (b) => b.serverUrl === params.serverUrl,
      )
      if (existingBackend) {
        error.value = `A connection to ${params.serverUrl} already exists`
        return null
      }

      const identity = await identityStore.getIdentityAsync(params.identityId)
      if (!identity) {
        throw new Error('Identity not found')
      }

      const presentation = await signClaimPresentation(
        identity.did,
        identity.publicKey,
        params.approvedClaims,
        identity.privateKey,
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

      const registerRes = await fetch(`${params.serverUrl}/identity-auth/register`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(registrationBody),
      })

      const registerData = await registerRes.json().catch(() => ({ error: 'Unknown error' }))

      if (!registerRes.ok) {
        if (registerRes.status !== 409 || registerData.error?.includes('another identity')) {
          throw new Error(`Registration failed: ${registerData.error || 'Unknown error'}`)
        }
        // 409 with 'DID already registered' = already registered and verified — proceed to login
        console.log('[SYNC] Already registered and verified, proceeding to login')
      } else if (registerData.status === 'verification_pending') {
        // New registration or re-registration (not yet verified)
        return {
          status: 'verification_pending' as const,
          did: registerData.did ?? identity.did,
          serverUrl: params.serverUrl,
          identityId: params.identityId,
          approvedClaims: params.approvedClaims,
        }
      }

      // Already registered and verified — complete the connection
      const backendId = await setupBackendAsync(params.serverUrl, params.identityId)
      if (!backendId) return null

      return { status: 'connected' as const, backendId }
    } catch (err) {
      console.error('[SYNC] Failed to create connection:', err)
      error.value = err instanceof Error ? err.message : 'Unknown error'
      return null
    } finally {
      isLoading.value = false
    }
  }

  /**
   * Verifies the email OTP code with the server.
   */
  const verifyEmailAsync = async (serverUrl: string, did: string, code: string): Promise<boolean> => {
    isLoading.value = true
    error.value = null

    try {
      const res = await fetch(`${serverUrl}/identity-auth/verify-email`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ did, code }),
      })

      if (!res.ok) {
        const errorData = await res.json().catch(() => ({ error: 'Unknown error' }))
        throw new Error(errorData.error || 'Verification failed')
      }

      return true
    } catch (err) {
      console.error('[SYNC] Email verification failed:', err)
      error.value = err instanceof Error ? err.message : 'Unknown error'
      return false
    } finally {
      isLoading.value = false
    }
  }

  /**
   * Resends the verification code.
   */
  const resendVerificationAsync = async (serverUrl: string, did: string): Promise<boolean> => {
    try {
      const res = await fetch(`${serverUrl}/identity-auth/resend-verification`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ did }),
      })

      if (!res.ok) {
        const errorData = await res.json().catch(() => ({ error: 'Unknown error' }))
        throw new Error(errorData.error || 'Failed to resend code')
      }

      return true
    } catch (err) {
      console.error('[SYNC] Resend verification failed:', err)
      error.value = err instanceof Error ? err.message : 'Unknown error'
      return false
    }
  }

  /**
   * Completes the connection after email verification:
   * login via challenge-response, create backend, ensure sync key, start sync.
   */
  const completeConnectionAsync = async (params: {
    serverUrl: string
    identityId: string
  }): Promise<string | null> => {
    isLoading.value = true
    error.value = null

    try {
      const backendId = await setupBackendAsync(params.serverUrl, params.identityId)
      return backendId
    } catch (err) {
      console.error('[SYNC] Failed to complete connection:', err)
      error.value = err instanceof Error ? err.message : 'Unknown error'
      return null
    } finally {
      isLoading.value = false
    }
  }

  /**
   * Internal: creates backend entry, logs in, sets up sync key, starts sync.
   */
  const setupBackendAsync = async (serverUrl: string, identityId: string): Promise<string | null> => {
    // Check if a backend with this URL already exists (e.g., reconnecting after data deletion)
    const existingBackend = await syncBackendsStore.findBackendByServerUrlAsync(serverUrl)

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
      const backendName = getBackendNameFromUrl(serverUrl)
      const tempBackend = await syncBackendsStore.addBackendAsync({
        name: backendName,
        serverUrl,
        enabled: false,
        vaultId: currentVaultId.value,
        identityId,
      })

      if (!tempBackend) {
        throw new Error('Failed to create backend entry')
      }

      backendId = tempBackend.id
      createdNew = true
    }

    try {
      console.log('[SYNC] Initializing Supabase client...')
      await syncEngineStore.initSupabaseClientAsync(backendId)

      if (!syncEngineStore.supabaseClient) {
        throw new Error('Supabase client not initialized')
      }

      console.log('[SYNC] Challenge-response login...')
      const session = await loginAsync(serverUrl, identityId)

      const { error: sessionError } =
        await syncEngineStore.supabaseClient.auth.setSession({
          access_token: session.access_token,
          refresh_token: session.refresh_token,
        })

      if (sessionError) {
        throw new Error(`Failed to set session: ${sessionError.message}`)
      }

      // Cache the token directly as workaround for Supabase getSession timing issues
      syncEngineStore.cacheAccessToken(session.access_token)

      console.log('[SYNC] Credentials verified successfully')

      if (!currentVaultPassword.value) {
        throw new Error('Vault password not available')
      }

      console.log('[SYNC] Ensuring vault sync key...')
      await syncEngineStore.ensureSyncKeyAsync(
        backendId,
        currentVaultId.value!,
        currentVaultName.value,
        currentVaultPassword.value,
        serverUrl,
      )

      await syncBackendsStore.updateBackendAsync(backendId, {
        enabled: true,
      })

      await syncBackendsStore.loadBackendsAsync()
      await syncOrchestratorStore.startSyncAsync()

      console.log('[SYNC] Connection created and sync started successfully')
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
    isLoading: readonly(isLoading),
    error: readonly(error),
    createConnectionAsync,
    verifyEmailAsync,
    resendVerificationAsync,
    completeConnectionAsync,
    fetchRequirementsAsync,
    loginAsync,
    getBackendNameFromUrl,
  }
}
