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

    <div class="hidden">
      <UiDialogConfirm
        v-model:open="showNewDeviceDialog"
        :confirm-label="t('newDevice.save')"
        :title="t('newDevice.title')"
        :description="t('newDevice.setName')"
        confirm-icon="mdi:content-save-outline"
        @abort="showNewDeviceDialog = false"
        @confirm="onSetDeviceNameAsync"
      >
        <template #body>
          <div class="flex flex-col gap-4">
            <p>{{ t('newDevice.intro') }}</p>
            <p>
              {{ t('newDevice.setName') }}
            </p>
            {{ deviceId }}
            <UiInput
              v-model="newDeviceName"
              :label="t('newDevice.label')"
              :rules="vaultDeviceNameSchema"
            />
          </div>
        </template>
      </UiDialogConfirm>
    </div>
  </div>
</template>

<script setup lang="ts">
definePageMeta({
  middleware: 'database',
})

const { t } = useI18n()
const route = useRoute()

const showNewDeviceDialog = ref(false)
const isWaitingForInitialSync = ref(false)
const syncProgress = ref<{ synced: number; total: number } | undefined>()

const { hostname } = storeToRefs(useDeviceStore())

const newDeviceName = ref<string>('unknown')

const { readNotificationsAsync } = useNotificationStore()
const { isKnownDeviceAsync, addDeviceNameAsync, setAsCurrentDeviceAsync } = useDeviceStore()
const { loadExtensionsAsync } = useExtensionsStore()
const { deviceId } = storeToRefs(useDeviceStore())
const { syncLocaleAsync, syncThemeAsync, syncVaultNameAsync } =
  useVaultSettingsStore()
const { syncDesktopIconSizeAsync } = useDesktopStore()
const { syncGradientVariantAsync, syncGradientEnabledAsync } = useGradientStore()
const syncOrchestratorStore = useSyncOrchestratorStore()
const syncBackendsStore = useSyncBackendsStore()

onMounted(async () => {
  try {
    // Check if this is a remote sync vault (coming from sync wizard)
    const isRemoteSync = route.query.remoteSync === 'true'

    if (isRemoteSync) {
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
      console.log('not known device')
      newDeviceName.value = hostname.value ?? 'unknown'
      showNewDeviceDialog.value = true
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

const { add } = useToast()
const onSetDeviceNameAsync = async () => {
  try {
    const check = vaultDeviceNameSchema.safeParse(newDeviceName.value)
    if (!check.success) {
      console.log('check failed', check.error)
      return
    }

    await addDeviceNameAsync({ name: newDeviceName.value })

    // Set this device as the current device in the vault
    await setAsCurrentDeviceAsync()

    showNewDeviceDialog.value = false
    add({ color: 'success', description: t('newDevice.success') })
  } catch (error) {
    console.error(error)
    add({ color: 'error', description: t('newDevice.error') })
  }
}
</script>

<i18n lang="yaml">
de:
  newDevice:
    title: Neues Gerät erkannt
    save: Speichern
    label: Name
    intro: Offenbar öffnest du das erste Mal diese Vault auf diesem Gerät.
    setName: Bitte gib diesem Gerät einen für dich sprechenden Namen. Dadurch kannst du später besser nachverfolgen, welche Änderungen von welchem Gerät erfolgt sind.
    success: Name erfolgreich gespeichert
    error: Name konnt nicht gespeichert werden

en:
  newDevice:
    title: New device recognized
    save: Save
    label: Name
    intro: This is obviously your first time with this Vault on this device.
    setName: Please give this device a name that is meaningful to you. This will make it easier for you to track which changes have been made by which device.
    success: Name successfully saved
    error: Name could not be saved
</i18n>
