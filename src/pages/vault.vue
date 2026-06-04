<template>
  <div>
    <!-- Remote Sync Overlay (only for initial server sync, not local vault init) -->
    <HaexSyncInitialSyncOverlay
      :is-visible="isWaitingForInitialSync"
      :progress="syncProgress"
    />

    <template v-if="isVaultReady">
      <NuxtLayout>
        <NuxtPage />
      </NuxtLayout>
      <HaexWelcomeDialog />
      <HaexDeviceReconciliationSpacePublishingDialog />
    </template>
  </div>
</template>

<script setup lang="ts">
import { and, eq } from 'drizzle-orm'
import { haexVaultSettings } from '~/database/schemas'
import { VaultSettingsKeyEnum } from '~/config/vault-settings'

definePageMeta({
  middleware: 'database',
})

const route = useRoute()

const isVaultReady = ref(false)
const isWaitingForInitialSync = ref(false)
const syncProgress = ref<{ synced: number; total: number } | undefined>()
const isRemoteSyncVault = computed(() => route.query.remoteSync === 'true')

const { readNotificationsAsync } = useNotificationStore()
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
    isVaultReady.value = true

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

    // Auto-start P2P endpoint unless the user explicitly disabled it on this device.
    // Default-on semantics: missing row = enabled; only 'false' disables.
    //
    // We gate on `deviceRowId` (haex_devices.id) — when the open vault has no
    // matching row yet, resolveAsync left the identity pending and the
    // Reconciliation dialog will pick it up. A watcher inside the dialog
    // restarts this autostart once the user confirms.
    const deviceStore = useDeviceStore()
    let autostartEnabled = false
    if (deviceStore.deviceRowId) {
      const peerAutostart = await currentVault.value?.drizzle.query.haexVaultSettings.findFirst({
        where: and(
          eq(haexVaultSettings.key, VaultSettingsKeyEnum.peerStorageAutostart),
          eq(haexVaultSettings.deviceId, deviceStore.deviceId),
        ),
      })
      autostartEnabled = peerAutostart?.value !== 'false'
      if (autostartEnabled) {
        usePeerStorageStore().startAsync().catch((error) => {
          console.warn('[P2P] Autostart failed:', error)
        })
      }
    }

    // Set up file sync event listeners so progress/complete events are handled.
    // When P2P is enabled, startAsync() calls loadRulesAsync() + startEnabledRulesAsync()
    // after the endpoint is up — starting rules here too would cause a double-start race.
    // When P2P is disabled (or the device is still pending reconciliation),
    // start rules here since startAsync() will not run.
    const fileSyncStore = useFileSyncStore()
    fileSyncStore.loadRulesAsync()
      .then(() => fileSyncStore.setupEventListeners())
      .then(() => {
        if (!autostartEnabled) {
          return fileSyncStore.startEnabledRulesAsync()
        }
      })
      .catch((error) => {
        console.warn('[FileSync] Setup failed:', error)
      })
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
