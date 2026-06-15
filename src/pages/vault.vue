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

// Watcher created after `await` in onMounted() — Vue 3 does not auto-bind it to
// the component lifecycle, so we hold the stop handle and dispose it explicitly
// in onBeforeUnmount to avoid duplicate startups and leaks across remounts.
let stopDeviceRowWatch: (() => void) | null = null

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

// Releasing the Rust-side mount lock + DB connection on page leave so that
// navigating away from a vault (back to the index, switching vaults, deleting
// the file externally before opening another) doesn't leave the vault_lock
// orphaned — the next `create_encrypted_database` would otherwise fail with
// VaultAlreadyMountedInProcess against a file the user no longer cares about.
//
// `onBeforeRouteLeave` is the primary path because vue-router awaits its
// returned promise before navigation proceeds — so the Rust-side
// `close_database` (which releases the lock) completes before the next
// vault page can mount. `onBeforeUnmount` stays as a fallback for non-routed
// teardowns (HMR, app close) but is fire-and-forget by Vue.
onBeforeRouteLeave(async () => {
  try {
    await vaultStore.closeAsync()
  } catch (error) {
    console.error('vault route-leave close failed:', error)
  }
})

onBeforeUnmount(async () => {
  stopDeviceRowWatch?.()
  stopDeviceRowWatch = null
  try {
    await vaultStore.closeAsync()
  } catch (error) {
    console.error('vault unmount close failed:', error)
  }
})

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
    // Start gates on `deviceRowId` (haex_devices.id). On a fresh vault,
    // `resolveAsync()` returns 'pending' and the device row only appears once
    // the user completes the Welcome/reconciliation dialog — so a one-shot
    // gate at mount would never fire and the endpoint would stay down for the
    // whole session (targeted invites then silently connect-timeout until the
    // next app launch). We therefore react to `deviceRowId` becoming available
    // instead of gating only on its mount-time value.
    const deviceStore = useDeviceStore()
    const peerStorageStore = usePeerStorageStore()

    const peerAutostart = await currentVault.value?.drizzle.query.haexVaultSettings.findFirst({
      where: and(
        eq(haexVaultSettings.key, VaultSettingsKeyEnum.peerStorageAutostart),
        eq(haexVaultSettings.deviceId, deviceStore.deviceId),
      ),
    })
    const autostartEnabled = peerAutostart?.value !== 'false'

    const tryStartPeerStorageAsync = async () => {
      if (!autostartEnabled || !deviceStore.deviceRowId || peerStorageStore.running) return
      try {
        await peerStorageStore.startAsync()
      } catch (error) {
        console.warn('[P2P] Autostart failed:', error)
      }
    }

    await tryStartPeerStorageAsync()
    // Covers the fresh-vault path: the device row is committed later by the
    // reconciliation dialog (registerNewAsync / reclaimAsync), after this
    // onMounted block has already run.
    stopDeviceRowWatch?.()
    stopDeviceRowWatch = watch(() => deviceStore.deviceRowId, (rowId) => {
      if (rowId) void tryStartPeerStorageAsync()
    })

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
