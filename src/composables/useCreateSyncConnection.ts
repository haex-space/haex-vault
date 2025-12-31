/**
 * Composable for creating a new sync connection to the current vault.
 * Used by both the Welcome Wizard and the Sync Settings view.
 */
export const useCreateSyncConnection = () => {
  const syncBackendsStore = useSyncBackendsStore()
  const syncEngineStore = useSyncEngineStore()
  const syncOrchestratorStore = useSyncOrchestratorStore()
  const vaultStore = useVaultStore()
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
   * Creates a new sync connection to the current vault.
   *
   * This function:
   * 1. Creates a temporary backend entry (disabled)
   * 2. Initializes Supabase client
   * 3. Verifies credentials by signing in
   * 4. Enables the backend
   * 5. Ensures sync key (creates vault on server if needed)
   * 6. Starts sync
   *
   * @param credentials - Server URL, email, and password
   * @returns The created backend ID on success, null on failure
   */
  const createConnectionAsync = async (credentials: {
    serverUrl: string
    email: string
    password: string
  }): Promise<string | null> => {
    isLoading.value = true
    error.value = null

    try {
      const { backends } = storeToRefs(syncBackendsStore)

      // Check if we already have a connection to this server
      const existingBackend = backends.value.find(
        (b) => b.serverUrl === credentials.serverUrl,
      )

      if (existingBackend) {
        error.value = `A connection to ${credentials.serverUrl} already exists`
        return null
      }

      const backendName = getBackendNameFromUrl(credentials.serverUrl)

      // 1. Create a temporary backend entry (disabled until verified)
      const tempBackend = await syncBackendsStore.addBackendAsync({
        name: backendName,
        serverUrl: credentials.serverUrl,
        email: credentials.email,
        password: credentials.password,
        enabled: false,
        vaultId: currentVaultId.value,
      })

      if (!tempBackend) {
        throw new Error('Failed to create backend entry')
      }

      const backendId = tempBackend.id

      try {
        // 2. Initialize Supabase client with the backend ID
        console.log('[SYNC] Initializing Supabase client...')
        await syncEngineStore.initSupabaseClientAsync(backendId)

        // 3. Verify credentials by signing in via server-side endpoint (bypasses Turnstile)
        if (!syncEngineStore.supabaseClient) {
          throw new Error('Supabase client not initialized')
        }

        const loginResponse = await fetch(`${credentials.serverUrl}/auth/login`, {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({
            email: credentials.email,
            password: credentials.password,
          }),
        })

        if (!loginResponse.ok) {
          const errorData = await loginResponse.json()
          throw new Error(`Authentication failed: ${errorData.error || 'Unknown error'}`)
        }

        const loginData = await loginResponse.json()

        // Set the session from the server response
        await syncEngineStore.supabaseClient.auth.setSession({
          access_token: loginData.access_token,
          refresh_token: loginData.refresh_token,
        })

        console.log('[SYNC] Credentials verified successfully')

        // 4. Ensure sync key FIRST (creates vault on server if it doesn't exist)
        // This MUST happen before enabling the backend, because enabling triggers
        // loadBackendsAsync() which fires DIRTY_TABLES events that start a push.
        // The push will fail with FK constraint error if vault key isn't on server yet.
        if (!currentVaultPassword.value) {
          throw new Error('Vault password not available')
        }
        await syncEngineStore.ensureSyncKeyAsync(
          backendId,
          currentVaultId.value!,
          currentVaultName.value,
          currentVaultPassword.value,
          undefined,
          credentials.password,
        )

        // 5. Enable the backend now that vault key is on server
        await syncBackendsStore.updateBackendAsync(backendId, {
          enabled: true,
        })

        // 6. Reload backends (triggers DIRTY_TABLES → push, but vault key is ready)
        await syncBackendsStore.loadBackendsAsync()

        // 7. Start sync
        await syncOrchestratorStore.startSyncAsync()

        console.log('[SYNC] Connection created and sync started successfully')

        return backendId
      } catch (err) {
        // If authentication fails, delete the backend entry
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
    getBackendNameFromUrl,
  }
}
