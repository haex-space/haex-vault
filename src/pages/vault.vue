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

    <!-- Welcome Dialog for new devices -->
    <HaexWelcomeDialog
      v-model:open="showWelcomeDialog"
      :initial-device-name="initialDeviceName"
      :is-connected-to-remote="isRemoteSyncVault"
      @complete="onWelcomeComplete"
    />
  </div>
</template>

<script setup lang="ts">
definePageMeta({
  middleware: 'database',
})

const route = useRoute()

const showWelcomeDialog = ref(false)
const initialDeviceName = ref<string>('unknown')
const isWaitingForInitialSync = ref(false)
const syncProgress = ref<{ synced: number; total: number } | undefined>()
const isRemoteSyncVault = computed(() => route.query.remoteSync === 'true')

const { hostname } = storeToRefs(useDeviceStore())

const { readNotificationsAsync } = useNotificationStore()
const { isKnownDeviceAsync, setAsCurrentDeviceAsync } = useDeviceStore()
const { loadExtensionsAsync } = useExtensionsStore()
const { syncLocaleAsync, syncThemeAsync, syncVaultNameAsync } =
  useVaultSettingsStore()
const { syncDesktopIconSizeAsync } = useDesktopStore()
const { syncGradientVariantAsync, syncGradientEnabledAsync } = useGradientStore()
const syncOrchestratorStore = useSyncOrchestratorStore()
const syncBackendsStore = useSyncBackendsStore()

onMounted(async () => {
  try {
    if (isRemoteSyncVault.value) {
      // Remote sync mode: Wait for initial sync to complete
      console.log('🔄 Remote sync mode detected - waiting for initial sync')
      isWaitingForInitialSync.value = true

      // Wait for backend to be configured and initial sync to complete
      await waitForInitialSyncAsync()

      isWaitingForInitialSync.value = false
      console.log('✅ Initial sync complete')

      // Load sync backends that were synced from remote vault
      const syncBackendsStore = useSyncBackendsStore()
      await syncBackendsStore.loadBackendsAsync()
      console.log('✅ Loaded sync backends from synced data')
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

    const knownDevice = await isKnownDeviceAsync()

    if (!knownDevice) {
      console.log('New device detected - showing welcome dialog')
      initialDeviceName.value = hostname.value ?? 'unknown'
      showWelcomeDialog.value = true
    } else {
      // Device is known, set it as current device
      await setAsCurrentDeviceAsync()
    }
  } catch (error) {
    console.error('vault mount error:', error)
  }
})

const waitForInitialSyncAsync = async () => {
  return new Promise<void>((resolve) => {
    // Poll sync state every 500ms
    const checkInterval = setInterval(() => {
      const backends = syncBackendsStore.enabledBackends

      if (backends.length === 0) {
        // No backends yet, keep waiting
        return
      }

      // Check if any backend is still syncing
      const syncStates = syncOrchestratorStore.syncStates
      const anySyncing = backends.some(backend => syncStates[backend.id]?.isSyncing)

      if (!anySyncing) {
        // All backends have completed initial sync
        clearInterval(checkInterval)
        resolve()
      }
    }, 500)

    // Timeout after 30 seconds
    setTimeout(() => {
      clearInterval(checkInterval)
      console.warn('Initial sync timeout - proceeding anyway')
      resolve()
    }, 30000)
  })
}

const onWelcomeComplete = () => {
  console.log('Welcome wizard completed')
}
</script>

<i18n lang="yaml">
de: {}
en: {}
</i18n>
