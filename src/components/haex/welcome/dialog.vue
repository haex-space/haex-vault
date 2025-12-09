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
              class="flex items-start gap-3 p-3 rounded-lg border border-default"
            >
              <UCheckbox
                :model-value="selectedExtensions.includes(ext.slug)"
                @update:model-value="toggleExtension(ext.slug)"
              />
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2 flex-wrap">
                  <span class="font-medium">{{ ext.name }}</span>
                  <UBadge
                    v-if="ext.isRecommended"
                    color="primary"
                    variant="subtle"
                    size="xs"
                  >
                    {{ t('steps.extensions.recommended') }}
                  </UBadge>
                  <UBadge
                    v-if="ext.installedVersion"
                    color="neutral"
                    variant="subtle"
                    size="xs"
                  >
                    {{ t('steps.extensions.installed', { version: ext.installedVersion }) }}
                  </UBadge>
                  <UBadge
                    v-if="ext.installedVersion && ext.availableVersions.length > 0 && ext.installedVersion !== ext.availableVersions[0]?.version"
                    color="warning"
                    variant="subtle"
                    size="xs"
                  >
                    {{ t('steps.extensions.updateAvailable') }}
                  </UBadge>
                </div>
                <p class="text-sm text-muted line-clamp-2">
                  {{ ext.shortDescription }}
                </p>

                <!-- Version Selection & Permissions Row -->
                <div
                  v-if="selectedExtensions.includes(ext.slug)"
                  class="flex items-center gap-2 mt-2"
                >
                  <!-- Version Dropdown -->
                  <USelectMenu
                    v-model="extensionVersionSelections[ext.slug]"
                    :items="getVersionOptionsForExtension(ext)"
                    :loading="ext.isLoadingVersions"
                    :placeholder="t('steps.extensions.selectVersion')"
                    size="xs"
                    class="w-32"
                    value-key="value"
                  />

                  <!-- Permissions Button -->
                  <UButton
                    size="xs"
                    color="neutral"
                    variant="ghost"
                    icon="i-heroicons-shield-check"
                    @click.stop="openPermissionsDialog(ext)"
                  >
                    {{ t('steps.extensions.permissions') }}
                  </UButton>
                </div>
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
          {{ currentStep === lastStepIndex ? t('actions.finish') : t('actions.next') }}
        </UButton>
      </div>
    </template>
  </UiDrawerModal>

  <!-- Permissions Dialog -->
  <HaexExtensionDialogInstall
    v-model:open="showPermissionsDialog"
    v-model:preview="currentPermissionsPreview"
    @confirm="onPermissionsConfirm"
    @deny="onPermissionsDeny"
  />
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
  /** If true, skip the sync step (already connected to remote vault) */
  isConnectedToRemote?: boolean
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

// Total steps depends on whether we're connected to remote
const totalSteps = computed(() => props.isConnectedToRemote ? 2 : 3)
const lastStepIndex = computed(() => totalSteps.value - 1)

const stepItems = computed(() => {
  const items = [
    { title: t('steps.device.title'), icon: 'i-heroicons-device-phone-mobile' },
    { title: t('steps.extensions.title'), icon: 'i-heroicons-puzzle-piece' },
  ]

  // Only add sync step if not already connected to remote
  if (!props.isConnectedToRemote) {
    items.push({ title: t('steps.sync.title'), icon: 'i-heroicons-cloud' })
  }

  return items
})

const currentStepTitle = computed(() => {
  switch (currentStep.value) {
    case 0:
      return t('steps.device.title')
    case 1:
      return t('steps.extensions.title')
    case 2:
      return props.isConnectedToRemote ? t('title') : t('steps.sync.title')
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
  installedVersion: string | null
  availableVersions: { version: string; label: string }[]
  isLoadingVersions: boolean
}

const recommendedExtensions = ref<RecommendedExtension[]>([])

// Track selected version per extension (slug -> version)
const extensionVersionSelections = reactive<Record<string, string>>({})

// Track custom permissions per extension (slug -> permissions)
const extensionPermissions = reactive<Record<string, import('~~/src-tauri/bindings/ExtensionPermissions').ExtensionPermissions | null>>({})

const loadRecommendedExtensionsAsync = async () => {
  isLoadingExtensions.value = true
  try {
    // Load installed extensions first
    await extensionStore.loadExtensionsAsync()

    await marketplace.fetchExtensions({
      page: 1,
      limit: 20,
      sort: 'downloads',
    })

    recommendedExtensions.value = marketplace.extensions.value.map((ext) => {
      // Check if this extension is already installed (by name match)
      const installedExt = extensionStore.availableExtensions.find(
        (installed) => installed.name === ext.name,
      )

      return {
        slug: ext.slug,
        name: ext.name,
        shortDescription: ext.shortDescription,
        iconUrl: ext.iconUrl,
        isRecommended: RECOMMENDED_EXTENSION_SLUGS.includes(ext.slug),
        installedVersion: installedExt?.version || null,
        availableVersions: [],
        isLoadingVersions: false,
      }
    })

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

    // Load versions for pre-selected extensions
    for (const slug of selectedExtensions.value) {
      loadVersionsForExtension(slug)
    }
  } catch (error) {
    console.error('Failed to load extensions:', error)
  } finally {
    isLoadingExtensions.value = false
  }
}

// Load available versions for an extension
const loadVersionsForExtension = async (slug: string) => {
  const ext = recommendedExtensions.value.find((e) => e.slug === slug)
  if (!ext || ext.availableVersions.length > 0) return // Already loaded

  ext.isLoadingVersions = true
  try {
    const versions = await marketplace.fetchVersions(slug)

    // Filter versions: only >= installed version (no downgrade)
    const filteredVersions = versions.filter((v) => {
      if (!ext.installedVersion) return true
      return extensionStore.compareVersions(v.version, ext.installedVersion) >= 0
    })

    ext.availableVersions = filteredVersions.map((v) => ({
      version: v.version,
      label: v.version === versions[0]?.version ? `${v.version} (${t('steps.extensions.latest')})` : v.version,
    }))

    // Set default selection to latest version
    if (ext.availableVersions.length > 0 && !extensionVersionSelections[slug]) {
      extensionVersionSelections[slug] = ext.availableVersions[0]!.version
    }
  } catch (error) {
    console.error(`Failed to load versions for ${slug}:`, error)
    ext.availableVersions = []
  } finally {
    ext.isLoadingVersions = false
  }
}

// Get version options for dropdown
const getVersionOptionsForExtension = (ext: RecommendedExtension) => {
  return ext.availableVersions.map((v) => ({
    value: v.version,
    label: v.label,
  }))
}

const toggleExtension = (slug: string) => {
  const index = selectedExtensions.value.indexOf(slug)
  if (index === -1) {
    selectedExtensions.value.push(slug)
    // Load versions when selecting
    loadVersionsForExtension(slug)
  } else {
    selectedExtensions.value.splice(index, 1)
    // Clean up selections
    // eslint-disable-next-line @typescript-eslint/no-dynamic-delete
    delete extensionVersionSelections[slug]
    // eslint-disable-next-line @typescript-eslint/no-dynamic-delete
    delete extensionPermissions[slug]
  }
}

// Permissions dialog
const showPermissionsDialog = ref(false)
const currentPermissionsExtension = ref<RecommendedExtension | null>(null)
const currentPermissionsPreview = ref<import('~~/src-tauri/bindings/ExtensionPreview').ExtensionPreview | null>(null)
const isLoadingPermissions = ref(false)

const openPermissionsDialog = async (ext: RecommendedExtension) => {
  currentPermissionsExtension.value = ext
  isLoadingPermissions.value = true
  showPermissionsDialog.value = true

  try {
    const selectedVersion = extensionVersionSelections[ext.slug]
    const downloadInfo = await marketplace.getDownloadUrl(ext.slug, selectedVersion)
    await extensionStore.downloadAndPreviewAsync(downloadInfo.downloadUrl, downloadInfo.bundleHash)
    currentPermissionsPreview.value = extensionStore.preview || null

    // If we already have custom permissions for this extension, apply them
    if (extensionPermissions[ext.slug] && currentPermissionsPreview.value) {
      currentPermissionsPreview.value.editablePermissions = extensionPermissions[ext.slug]!
    }
  } catch (error) {
    console.error('Failed to load extension preview:', error)
    add({ color: 'error', description: t('errors.loadPermissions') })
    showPermissionsDialog.value = false
  } finally {
    isLoadingPermissions.value = false
  }
}

const onPermissionsConfirm = () => {
  if (currentPermissionsExtension.value && currentPermissionsPreview.value) {
    // Save custom permissions
    extensionPermissions[currentPermissionsExtension.value.slug] = currentPermissionsPreview.value.editablePermissions
  }
  showPermissionsDialog.value = false
  extensionStore.clearPendingInstall()
}

const onPermissionsDeny = () => {
  showPermissionsDialog.value = false
  extensionStore.clearPendingInstall()
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
  // If connected to remote, finish immediately. Otherwise go to sync step.
  if (props.isConnectedToRemote) {
    finishWizardAsync({ withSync: false })
  } else {
    currentStep.value = 2
  }
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
    // If connected to remote, finish. Otherwise go to sync step.
    if (props.isConnectedToRemote) {
      await finishWizardAsync({ withSync: false })
    } else {
      currentStep.value = 2
    }
  } else if (currentStep.value === 2) {
    // Complete the wizard with sync
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
      // Get selected version (or undefined for latest)
      const selectedVersion = extensionVersionSelections[slug]

      // Get download URL from marketplace API with specific version
      const downloadInfo = await marketplace.getDownloadUrl(slug, selectedVersion)

      // Download and preview
      await extensionStore.downloadAndPreviewAsync(
        downloadInfo.downloadUrl,
        downloadInfo.bundleHash,
      )

      const previewManifest = extensionStore.preview?.manifest
      if (!previewManifest) {
        console.error(`No manifest for ${slug}`)
        continue
      }

      // Use custom permissions if user modified them, otherwise use default from preview
      const permissions = extensionPermissions[slug] || extensionStore.preview?.editablePermissions

      // Check if extension files are already installed locally
      const isLocallyInstalled = await extensionStore.isExtensionInstalledAsync({
        publicKey: previewManifest.publicKey,
        name: previewManifest.name,
        version: previewManifest.version,
      })

      let extensionId: string | undefined

      if (isLocallyInstalled) {
        // Already fully installed - skip
        console.log(`Extension ${slug} already installed locally, skipping`)
        extensionStore.clearPendingInstall()
        continue
      }

      // Check if extension exists in DB (e.g., from sync) but not locally installed
      const existingExt = extensionStore.availableExtensions.find(
        (ext) => ext.publicKey === previewManifest.publicKey && ext.name === previewManifest.name,
      )

      if (existingExt) {
        // Extension exists in DB from sync - only install files
        console.log(`Extension ${slug} exists in DB, installing files only`)
        extensionId = await extensionStore.installFilesAsync(existingExt.id)
      } else {
        // New extension - full installation (DB + files)
        extensionId = await extensionStore.installPendingAsync(permissions)
      }

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
      const ext = recommendedExtensions.value.find((e) => e.slug === slug)
      add({
        color: 'error',
        title: t('errors.extensionInstall', { name: ext?.name || slug }),
        description: (error as { message?: string })?.message || String(error),
      })
      extensionStore.clearPendingInstall()
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
      installed: "Installiert: v{version}"
      updateAvailable: Update verfügbar
      selectVersion: Version
      latest: Neueste
      permissions: Berechtigungen
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
    loadPermissions: Berechtigungen konnten nicht geladen werden
    extensionInstall: "Fehler bei Installation von {name}"

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
      installed: "Installed: v{version}"
      updateAvailable: Update available
      selectVersion: Version
      latest: Latest
      permissions: Permissions
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
    loadPermissions: Could not load permissions
    extensionInstall: "Failed to install {name}"
</i18n>
