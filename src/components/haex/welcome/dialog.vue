<template>
  <UiDrawerModal
    v-model:open="open"
    :title="currentStepTitle"
  >
    <template #content>
      <div class="flex flex-col gap-6">
        <!-- Progress Stepper -->
        <UStepper
          v-model="currentStep"
          :items="stepItems"
          size="sm"
        />

        <!-- Step 1: Device Name -->
        <div
          v-if="currentStep === 0"
          class="space-y-4"
        >
          <div class="text-center space-y-2">
            <UIcon
              name="i-heroicons-device-phone-mobile"
              class="w-12 h-12 text-primary mx-auto"
            />
            <p class="text-muted">
              {{ t('steps.device.description') }}
            </p>
          </div>

          <UiInput
            v-model="deviceName"
            :label="t('steps.device.label')"
            :placeholder="t('steps.device.placeholder')"
            :rules="vaultDeviceNameSchema"
            size="xl"
            class="w-full"
          />
        </div>

        <!-- Step 2: Extension Selection -->
        <div
          v-if="currentStep === 1"
          class="space-y-4"
        >
          <div class="text-center space-y-2">
            <UIcon
              name="i-heroicons-puzzle-piece"
              class="w-12 h-12 text-primary mx-auto"
            />
            <p class="text-muted">
              {{ t('steps.extensions.description') }}
            </p>
          </div>

          <!-- Loading -->
          <div
            v-if="isLoadingExtensions"
            class="flex justify-center py-8"
          >
            <UIcon
              name="i-heroicons-arrow-path"
              class="w-8 h-8 animate-spin text-muted"
            />
          </div>

          <!-- Extension List -->
          <div
            v-else
            class="space-y-3 max-h-80 overflow-y-auto"
          >
            <div
              v-for="ext in recommendedExtensions"
              :key="ext.slug"
              class="flex items-start gap-3 p-3 rounded-lg border border-default hover:bg-elevated transition-colors cursor-pointer"
              @click="toggleExtension(ext.slug)"
            >
              <UCheckbox
                :model-value="selectedExtensions.includes(ext.slug)"
                @click.stop
              />
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <span class="font-medium">{{ ext.name }}</span>
                  <UBadge
                    v-if="ext.isRecommended"
                    color="primary"
                    variant="subtle"
                    size="xs"
                  >
                    {{ t('steps.extensions.recommended') }}
                  </UBadge>
                </div>
                <p class="text-sm text-muted line-clamp-2">
                  {{ ext.shortDescription }}
                </p>
              </div>
              <img
                v-if="ext.iconUrl"
                :src="ext.iconUrl"
                :alt="ext.name"
                class="w-10 h-10 rounded-lg object-cover shrink-0"
              >
            </div>

            <p
              v-if="recommendedExtensions.length === 0"
              class="text-center text-muted py-4"
            >
              {{ t('steps.extensions.noExtensions') }}
            </p>
          </div>

          <p class="text-xs text-muted text-center">
            {{ t('steps.extensions.hint') }}
          </p>
        </div>

        <!-- Step 3: Sync Server -->
        <div
          v-if="currentStep === 2"
          class="space-y-4"
        >
          <div class="text-center space-y-2">
            <UIcon
              name="i-heroicons-cloud"
              class="w-12 h-12 text-primary mx-auto"
            />
            <p class="text-muted">
              {{ t('steps.sync.description') }}
            </p>
          </div>

          <!-- Sync Form -->
          <HaexSyncAddBackend
            v-model:server-url="syncCredentials.serverUrl"
            v-model:email="syncCredentials.email"
            v-model:password="syncCredentials.password"
            :items="serverOptions"
            :is-loading="isSyncLoading"
          />
        </div>
      </div>
    </template>

    <template #footer>
      <div class="flex gap-3 w-full">
        <!-- Back Button -->
        <UButton
          v-if="currentStep > 0"
          color="neutral"
          variant="outline"
          @click="previousStep"
        >
          {{ t('actions.back') }}
        </UButton>

        <div class="flex-1" />

        <!-- Skip Button (Step 2: Extensions) -->
        <UButton
          v-if="currentStep === 1"
          color="neutral"
          variant="ghost"
          @click="skipExtensions"
        >
          {{ t('actions.skip') }}
        </UButton>

        <!-- Skip Button (Step 3: Sync) -->
        <UButton
          v-if="currentStep === 2"
          color="neutral"
          variant="ghost"
          :disabled="isProcessing"
          @click="skipSync"
        >
          {{ t('actions.skip') }}
        </UButton>

        <!-- Next/Finish Button -->
        <UButton
          color="primary"
          :loading="isProcessing"
          :disabled="!canProceed"
          @click="nextStep"
        >
          {{ currentStep === 2 ? t('actions.finish') : t('actions.next') }}
        </UButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import { useMarketplace } from '@haex-space/marketplace-sdk/vue'

const { t } = useI18n()
const { add } = useToast()

const open = defineModel<boolean>('open', { default: false })

const emit = defineEmits<{
  complete: []
}>()

// Props
const props = defineProps<{
  initialDeviceName?: string
}>()

// Stores
const { hostname } = storeToRefs(useDeviceStore())
const { addDeviceNameAsync, setAsCurrentDeviceAsync } = useDeviceStore()
const extensionStore = useExtensionsStore()
const { serverOptions } = useSyncServerOptions()

// Sync connection composable
const { isLoading: isSyncLoading, error: syncError, createConnectionAsync } =
  useCreateSyncConnection()

// Step management
const currentStep = ref(0)
const stepItems = computed(() => [
  { title: t('steps.device.title'), icon: 'i-heroicons-device-phone-mobile' },
  { title: t('steps.extensions.title'), icon: 'i-heroicons-puzzle-piece' },
  { title: t('steps.sync.title'), icon: 'i-heroicons-cloud' },
])

const currentStepTitle = computed(() => {
  switch (currentStep.value) {
    case 0:
      return t('steps.device.title')
    case 1:
      return t('steps.extensions.title')
    case 2:
      return t('steps.sync.title')
    default:
      return t('title')
  }
})

// Step 1: Device Name
const deviceName = ref(props.initialDeviceName || hostname.value || 'unknown')

// Step 2: Extensions
const marketplace = useMarketplace()
const isLoadingExtensions = ref(false)
const selectedExtensions = ref<string[]>([])

// Recommended extension slugs - these will be pre-selected
const RECOMMENDED_EXTENSION_SLUGS = [
  'haex-pass',
]

interface RecommendedExtension {
  slug: string
  name: string
  shortDescription: string
  iconUrl: string | null
  isRecommended: boolean
}

const recommendedExtensions = ref<RecommendedExtension[]>([])

const loadRecommendedExtensionsAsync = async () => {
  isLoadingExtensions.value = true
  try {
    await marketplace.fetchExtensions({
      page: 1,
      limit: 20,
      sort: 'downloads',
    })

    recommendedExtensions.value = marketplace.extensions.value.map((ext) => ({
      slug: ext.slug,
      name: ext.name,
      shortDescription: ext.shortDescription,
      iconUrl: ext.iconUrl,
      isRecommended: RECOMMENDED_EXTENSION_SLUGS.includes(ext.slug),
    }))

    // Sort: recommended first, then by name
    recommendedExtensions.value.sort((a, b) => {
      if (a.isRecommended && !b.isRecommended) return -1
      if (!a.isRecommended && b.isRecommended) return 1
      return a.name.localeCompare(b.name)
    })

    // Pre-select recommended extensions
    selectedExtensions.value = recommendedExtensions.value
      .filter((ext) => ext.isRecommended)
      .map((ext) => ext.slug)
  } catch (error) {
    console.error('Failed to load extensions:', error)
  } finally {
    isLoadingExtensions.value = false
  }
}

const toggleExtension = (slug: string) => {
  const index = selectedExtensions.value.indexOf(slug)
  if (index === -1) {
    selectedExtensions.value.push(slug)
  } else {
    selectedExtensions.value.splice(index, 1)
  }
}

// Step 3: Sync
const syncCredentials = ref({
  serverUrl: 'https://sync.haex.space',
  email: '',
  password: '',
})

// Processing state
const isProcessing = ref(false)

// Validation
const canProceed = computed(() => {
  if (isProcessing.value) return false

  switch (currentStep.value) {
    case 0:
      return deviceName.value.trim().length >= 2
    case 1:
      return true // Extensions are optional
    case 2:
      // Sync credentials must be filled to connect
      return (
        syncCredentials.value.serverUrl.trim() !== '' &&
        syncCredentials.value.email.trim() !== '' &&
        syncCredentials.value.password.trim() !== ''
      )
    default:
      return false
  }
})

// Navigation
const previousStep = () => {
  if (currentStep.value > 0) {
    currentStep.value--
  }
}

const skipExtensions = () => {
  selectedExtensions.value = []
  currentStep.value = 2
}

const finishWizardAsync = async (options: { withSync: boolean }) => {
  isProcessing.value = true

  try {
    await saveDeviceNameAsync()
    await installSelectedExtensionsAsync()

    if (options.withSync) {
      await setupSyncAsync()
    }

    add({
      color: 'success',
      title: t('success.complete'),
      description: t('success.completeDescription'),
    })

    open.value = false
    emit('complete')
  } catch (error) {
    console.error('Failed to complete wizard:', error)
    add({ color: 'error', description: t('errors.complete') })
  } finally {
    isProcessing.value = false
  }
}

const skipSync = () => finishWizardAsync({ withSync: false })

const nextStep = async () => {
  if (currentStep.value === 0) {
    // Validate device name before proceeding
    const check = vaultDeviceNameSchema.safeParse(deviceName.value)
    if (!check.success) {
      add({ color: 'error', description: t('errors.invalidDeviceName') })
      return
    }
    currentStep.value = 1
    // Load extensions when entering step 2
    await loadRecommendedExtensionsAsync()
  } else if (currentStep.value === 1) {
    // Proceed to sync step
    currentStep.value = 2
  } else if (currentStep.value === 2) {
    // Complete the wizard
    await completeWizardAsync()
  }
}

// Actions
const saveDeviceNameAsync = async () => {
  try {
    const check = vaultDeviceNameSchema.safeParse(deviceName.value)
    if (!check.success) {
      add({ color: 'error', description: t('errors.invalidDeviceName') })
      return
    }

    await addDeviceNameAsync({ name: deviceName.value })
    await setAsCurrentDeviceAsync()
  } catch (error) {
    console.error('Failed to save device name:', error)
    add({ color: 'error', description: t('errors.saveDeviceName') })
    throw error
  }
}

const installSelectedExtensionsAsync = async () => {
  if (selectedExtensions.value.length === 0) return

  for (const slug of selectedExtensions.value) {
    try {
      // Get download URL from marketplace API
      const downloadInfo = await marketplace.getDownloadUrl(slug)

      // Download and install
      await extensionStore.downloadAndPreviewAsync(
        downloadInfo.downloadUrl,
        downloadInfo.bundleHash,
      )

      // Install the extension
      const extensionId = await extensionStore.installPendingAsync(
        extensionStore.preview?.editablePermissions,
      )

      // Add to desktop
      if (extensionId) {
        try {
          await useDesktopStore().addDesktopItemAsync('extension', extensionId)
        } catch {
          // Ignore desktop errors
        }
      }

      extensionStore.clearPendingInstall()
    } catch (error) {
      console.error(`Failed to install extension ${slug}:`, error)
      // Continue with next extension
    }
  }

  await extensionStore.loadExtensionsAsync()
}

const setupSyncAsync = async () => {
  const backendId = await createConnectionAsync({
    serverUrl: syncCredentials.value.serverUrl,
    email: syncCredentials.value.email,
    password: syncCredentials.value.password,
  })

  if (backendId) {
    add({
      color: 'success',
      title: t('success.syncConfigured'),
    })
  } else if (syncError.value) {
    add({
      color: 'error',
      title: t('errors.syncSetup'),
      description: syncError.value,
    })
  }
  // Don't throw - let the wizard complete even if sync fails
}

const completeWizardAsync = () => finishWizardAsync({ withSync: true })

// Watch for open state to reset
watch(open, (isOpen) => {
  if (isOpen) {
    currentStep.value = 0
    deviceName.value = props.initialDeviceName || hostname.value || 'unknown'
    selectedExtensions.value = []
    syncCredentials.value = {
      serverUrl: 'https://sync.haex.space',
      email: '',
      password: '',
    }
  }
})
</script>

<i18n lang="yaml">
de:
  title: Willkommen
  steps:
    device:
      title: Gerätename
      description: Bitte gib diesem Gerät einen Namen. So kannst du später besser nachverfolgen, welche Änderungen von welchem Gerät stammen.
      label: Gerätename
      placeholder: z.B. MacBook Pro, iPhone, Arbeits-PC
    extensions:
      title: Erweiterungen
      description: Wähle Erweiterungen aus, die du installieren möchtest. Empfohlene Erweiterungen sind bereits vorausgewählt.
      recommended: Empfohlen
      noExtensions: Keine Erweiterungen verfügbar
      hint: Du kannst später jederzeit weitere Erweiterungen im Marketplace installieren.
    sync:
      title: Synchronisierung
      description: Synchronisiere deine Vault mit anderen Geräten über einen Sync-Server.
      skipTitle: Später einrichten
      skipDescription: Die Synchronisierung kann jederzeit in den Einstellungen eingerichtet werden.
      enableTitle: Jetzt verbinden
      enableDescription: Verbinde dich mit einem Sync-Server, um deine Daten zu synchronisieren.
  actions:
    back: Zurück
    next: Weiter
    skip: Überspringen
    finish: Fertig
  success:
    complete: Einrichtung abgeschlossen
    completeDescription: Deine Vault ist jetzt einsatzbereit!
    syncConfigured: Synchronisierung eingerichtet
  errors:
    invalidDeviceName: Ungültiger Gerätename
    saveDeviceName: Gerätename konnte nicht gespeichert werden
    syncSetup: Synchronisierung konnte nicht eingerichtet werden
    complete: Einrichtung konnte nicht abgeschlossen werden

en:
  title: Welcome
  steps:
    device:
      title: Device Name
      description: Please give this device a name. This will help you track which changes came from which device.
      label: Device Name
      placeholder: e.g. MacBook Pro, iPhone, Work PC
    extensions:
      title: Extensions
      description: Choose extensions you want to install. Recommended extensions are pre-selected.
      recommended: Recommended
      noExtensions: No extensions available
      hint: You can always install more extensions from the Marketplace later.
    sync:
      title: Synchronization
      description: Sync your vault with other devices via a sync server.
      skipTitle: Set up later
      skipDescription: Synchronization can be set up anytime in settings.
      enableTitle: Connect now
      enableDescription: Connect to a sync server to synchronize your data.
  actions:
    back: Back
    next: Next
    skip: Skip
    finish: Finish
  success:
    complete: Setup Complete
    completeDescription: Your vault is now ready to use!
    syncConfigured: Sync configured
  errors:
    invalidDeviceName: Invalid device name
    saveDeviceName: Could not save device name
    syncSetup: Could not set up synchronization
    complete: Could not complete setup
</i18n>
