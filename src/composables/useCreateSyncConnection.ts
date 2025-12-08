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

        // 3. Verify credentials by signing in
        if (!syncEngineStore.supabaseClient) {
          throw new Error('Supabase client not initialized')
        }

        const { error: signInError } =
          await syncEngineStore.supabaseClient.auth.signInWithPassword({
            email: credentials.email,
            password: credentials.password,
          })

        if (signInError) {
          throw new Error(`Authentication failed: ${signInError.message}`)
        }

        console.log('[SYNC] Credentials verified successfully')

        // 4. Enable the backend now that credentials are verified
        await syncBackendsStore.updateBackendAsync(backendId, {
          enabled: true,
        })

        // 5. Reload backends
        await syncBackendsStore.loadBackendsAsync()

        // 6. Ensure sync key (creates vault on server if it doesn't exist)
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
