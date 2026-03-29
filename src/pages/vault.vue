<template>
  <div>
    <!-- Remote Sync Loading Overlay -->
    <HaexSyncInitialSyncOverlay
      :is-visible="isWaitingForInitialSync"
      :progress="syncProgress"
    />

    <NuxtLayout>
      <NuxtPage />
    </NuxtLayout>

  </div>
</template>

<script setup lang="ts">
definePageMeta({
  middleware: 'database',
})

const route = useRoute()

const isWaitingForInitialSync = ref(false)
const syncProgress = ref<{ synced: number; total: number } | undefined>()
const isRemoteSyncVault = computed(() => route.query.remoteSync === 'true')

import { and, eq } from 'drizzle-orm'
import { haexVaultSettings } from '~/database/schemas'
import { VaultSettingsKeyEnum } from '~/config/vault-settings'

const { readNotificationsAsync } = useNotificationStore()
const tourStore = useTourStore()
const { loadExtensionsAsync } = useExtensionsStore()
const { setupEventListeners: setupBroadcastListeners } = useExtensionBroadcastStore()
const { syncLocaleAsync, syncThemeAsync, syncVaultNameAsync } =
  useVaultSettingsStore()
const { syncDesktopIconSizeAsync } = useDesktopStore()
const { syncGradientVariantAsync, syncGradientEnabledAsync } = useGradientStore()
const syncOrchestratorStore = useSyncOrchestratorStore()
const syncBackendsStore = useSyncBackendsStore()
const vaultStore = useVaultStore()
const { currentVault } = storeToRefs(vaultStore)

// Initialize navigation store (registers popstate listener + boundary)
useNavigationStore()

onMounted(async () => {
  try {
    // Initialize vault (device, spaces, cleanup) — must run after navigation
    await vaultStore.initVaultAsync()

    if (isRemoteSyncVault.value) {
      // Remote sync mode: Wait for initial sync to complete
      isWaitingForInitialSync.value = true

      // Wait for backend to be configured and initial sync to complete
      await waitForInitialSyncAsync()

      isWaitingForInitialSync.value = false

      // Load sync backends that were synced from remote vault
      const syncBackendsStore = useSyncBackendsStore()
      await syncBackendsStore.loadBackendsAsync()
    }

    // Sync settings first before other initialization
    await Promise.allSettled([
      syncLocaleAsync(),
      syncThemeAsync(),
      syncVaultNameAsync(),
      syncDesktopIconSizeAsync(),
      syncGradientVariantAsync(),
      syncGradientEnabledAsync(),
      loadExtensionsAsync(),
      readNotificationsAsync(),
    ])

    // Initialize extension broadcast event listeners early so external requests
    // (from browser extensions via WebSocket bridge) can be forwarded to
    // extension iframes as soon as they mount — not only after the first
    // extension-frame.vue renders.
    setupBroadcastListeners()

    // Show onboarding tour for new vaults (no onboarding_completed setting)
    const onboarding = await currentVault.value?.drizzle.query.haexVaultSettings.findFirst({
      where: eq(haexVaultSettings.key, VaultSettingsKeyEnum.onboardingCompleted),
    })
    if (!onboarding?.value) {
      await currentVault.value?.drizzle.insert(haexVaultSettings).values({
        id: crypto.randomUUID(),
        key: VaultSettingsKeyEnum.onboardingCompleted,
        value: 'true',
      })
      await tourStore.start()
    }

    // Auto-start P2P endpoint if configured for this device
    const deviceStore = useDeviceStore()
    const peerAutostart = deviceStore.deviceId
      ? await currentVault.value?.drizzle.query.haexVaultSettings.findFirst({
          where: and(
            eq(haexVaultSettings.key, VaultSettingsKeyEnum.peerStorageAutostart),
            eq(haexVaultSettings.deviceId, deviceStore.deviceId),
          ),
        })
      : null
    if (peerAutostart?.value === 'true') {
      usePeerStorageStore().startAsync().catch((error) => {
        console.warn('[P2P] Autostart failed:', error)
      })
    }
  } catch (error) {
    console.error('vault mount error:', error)
  }
})

const waitForInitialSyncAsync = async () => {
  return new Promise<void>((resolve) => {
    let syncStarted = false

    // Poll sync state every 500ms
    const checkInterval = setInterval(() => {
      // For initial sync, we need to check the temporary backend state
      // The temporary backend is used during initial sync before being persisted to DB
      const tempBackend = syncBackendsStore.temporaryBackend
      const persistedBackends = syncBackendsStore.enabledBackends

      // If we have a temporary backend, check its sync state
      if (tempBackend) {
        const syncStates = syncOrchestratorStore.syncStates
        const tempState = syncStates[tempBackend.id]

        // Track when sync actually starts (has syncState with isSyncing=true)
        if (tempState?.isSyncing) {
          syncStarted = true
        }

        // Wait until sync has started AND completed
        // This prevents resolving before performInitialPullAsync() is even called
        if (syncStarted && tempState && !tempState.isSyncing) {
          clearInterval(checkInterval)
          resolve()
          return
        }

        // Keep waiting for temporary backend to start/finish syncing
        return
      }

      // If no temporary backend but we have persisted backends, check them
      if (persistedBackends.length > 0) {
        const syncStates = syncOrchestratorStore.syncStates
        const anySyncing = persistedBackends.some(backend => syncStates[backend.id]?.isSyncing)

        if (!anySyncing) {
          // All backends have completed initial sync
          clearInterval(checkInterval)
          resolve()
          return
        }
      }

      // No backends yet, keep waiting
    }, 500)

    // Timeout after 60 seconds
    setTimeout(() => {
      clearInterval(checkInterval)
      console.warn('Initial sync timeout - proceeding anyway')
      resolve()
    }, 60000)
  })
}

</script>

<i18n lang="yaml">
de: {}
en: {}
</i18n>
