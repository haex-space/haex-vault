import {
  importUserPrivateKeyAsync,
  encryptPrivateKeyAsync,
} from '@haex-space/vault-sdk'

export interface ServerRequirements {
  serverName: string
  claims: { type: string; required: boolean; label: string }[]
  didMethods: string[]
}

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

  /**
   * Gets a human-readable backend name from a server URL
   */
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

  /**
   * Fetches server requirements (required claims, supported DID methods).
   */
  const fetchRequirementsAsync = async (serverUrl: string): Promise<ServerRequirements> => {
    const res = await fetch(`${serverUrl}/identity-auth/requirements`)
    if (!res.ok) {
      const data = await res.json().catch(() => ({ error: 'Unknown error' }))
      throw new Error(`Failed to fetch requirements: ${data.error || res.statusText}`)
    }
    return res.json()
  }

  /**
   * Performs challenge-response login and sets the Supabase session.
   * Returns the session tokens.
   */
  const loginAsync = async (serverUrl: string, identityId: string): Promise<{ access_token: string; refresh_token: string }> => {
    const identity = await identityStore.getIdentityAsync(identityId)
    if (!identity) {
      throw new Error('Identity not found')
    }

    // 1. Request challenge
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

    // 2. Sign nonce with identity's private key
    const privateKey = await importUserPrivateKeyAsync(identity.privateKey)
    const sig = await crypto.subtle.sign(
      { name: 'ECDSA', hash: 'SHA-256' },
      privateKey,
      new TextEncoder().encode(nonce),
    )
    const signature = btoa(String.fromCharCode(...new Uint8Array(sig)))

    // 3. Verify and get JWT
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
   * Creates a new sync connection to the current vault using identity-based auth.
   *
   * This function:
   * 1. Signs a claim presentation with the user's identity
   * 2. Registers with the server via POST /identity-auth/register
   * 3. Logs in via challenge-response
   * 4. Creates sync backend entry
   * 5. Ensures sync key (creates vault on server if needed)
   * 6. Starts sync
   *
   * @returns The created backend ID on success, null on failure
   */
  const createConnectionAsync = async (params: {
    serverUrl: string
    identityId: string
    approvedClaims: Record<string, string>
  }): Promise<string | null> => {
    isLoading.value = true
    error.value = null

    try {
      const { backends } = storeToRefs(syncBackendsStore)

      // Check if we already have a connection to this server
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

      // 1. Sign claim presentation
      const presentation = await signClaimPresentation(
        identity.did,
        identity.publicKey,
        params.approvedClaims,
        identity.privateKey,
      )

      // 2. Build registration body
      const registrationBody: Record<string, unknown> = { presentation }

      // Encrypt private key for recovery if vault password is available
      if (currentVaultPassword.value) {
        try {
          const recovery = await encryptPrivateKeyAsync(
            identity.privateKey,
            currentVaultPassword.value,
          )
          registrationBody.encryptedPrivateKey = recovery.encryptedPrivateKey
          registrationBody.privateKeyNonce = recovery.nonce
          registrationBody.privateKeySalt = recovery.salt
        } catch (e) {
          console.warn('[SYNC] Could not encrypt private key for recovery:', e)
        }
      }

      // 3. Register with the server
      const registerRes = await fetch(`${params.serverUrl}/identity-auth/register`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(registrationBody),
      })

      if (!registerRes.ok) {
        const errorData = await registerRes.json().catch(() => ({ error: 'Unknown error' }))
        // 409 means already registered — that's ok, proceed to login
        if (registerRes.status !== 409) {
          throw new Error(`Registration failed: ${errorData.error || 'Unknown error'}`)
        }
        console.log('[SYNC] Already registered, proceeding to login')
      }

      const backendName = getBackendNameFromUrl(params.serverUrl)

      // 4. Create backend entry (disabled until verified)
      const tempBackend = await syncBackendsStore.addBackendAsync({
        name: backendName,
        serverUrl: params.serverUrl,
        enabled: false,
        vaultId: currentVaultId.value,
        identityId: params.identityId,
      })

      if (!tempBackend) {
        throw new Error('Failed to create backend entry')
      }

      const backendId = tempBackend.id

      try {
        // 5. Initialize Supabase client
        console.log('[SYNC] Initializing Supabase client...')
        await syncEngineStore.initSupabaseClientAsync(backendId)

        if (!syncEngineStore.supabaseClient) {
          throw new Error('Supabase client not initialized')
        }

        // 6. Login via challenge-response
        console.log('[SYNC] Challenge-response login...')
        const session = await loginAsync(params.serverUrl, params.identityId)

        // Set the session from the server response
        const { error: sessionError } =
          await syncEngineStore.supabaseClient.auth.setSession({
            access_token: session.access_token,
            refresh_token: session.refresh_token,
          })

        if (sessionError) {
          throw new Error(`Failed to set session: ${sessionError.message}`)
        }

        console.log('[SYNC] Credentials verified successfully')

        // 7. Ensure sync key FIRST (creates vault on server if it doesn't exist)
        if (!currentVaultPassword.value) {
          throw new Error('Vault password not available')
        }
        await syncEngineStore.ensureSyncKeyAsync(
          backendId,
          currentVaultId.value!,
          currentVaultName.value,
          currentVaultPassword.value,
        )

        // 8. Enable the backend now that vault key is on server
        await syncBackendsStore.updateBackendAsync(backendId, {
          enabled: true,
        })

        // 9. Reload backends
        await syncBackendsStore.loadBackendsAsync()

        // 10. Start sync
        await syncOrchestratorStore.startSyncAsync()

        console.log('[SYNC] Connection created and sync started successfully')

        return backendId
      } catch (err) {
        // If setup fails, delete the backend entry
        console.error('[SYNC] Connection setup failed, removing backend entry')
        await syncBackendsStore.deleteBackendAsync(backendId)
        throw err
      }
    } catch (err) {
      console.error('[SYNC] Failed to create connection:', err)
      error.value = err instanceof Error ? err.message : 'Unknown error'
      return null
    } finally {
      isLoading.value = false
    }
  }

  return {
    isLoading: readonly(isLoading),
    error: readonly(error),
    createConnectionAsync,
    fetchRequirementsAsync,
    loginAsync,
    getBackendNameFromUrl,
  }
}
