<template>
  <HaexSystem :is-dragging="isDragging">
    <template #header>
      <div
        class="flex flex-col @lg:flex-row @lg:items-center justify-between gap-4"
      >
        <div>
          <h1 class="text-2xl font-bold">
            {{ t('title') }}
          </h1>
          <p class="text-sm text-gray-500 dark:text-gray-400 mt-1">
            {{ t('subtitle') }}
          </p>
        </div>

        <div
          class="flex flex-col @lg:flex-row items-stretch @lg:items-center gap-3"
        >
          <!-- Install from File Button -->
          <UiButton
            :label="t('extension.installFromFile')"
            icon="i-heroicons-arrow-up-tray"
            color="neutral"
            block
            @click="onSelectExtensionAsync"
          />
        </div>
      </div>
    </template>

    <div class="flex flex-col h-full">
      <!-- Search and Filters -->
      <div
        class="flex flex-col @lg:flex-row items-stretch @lg:items-center gap-4 p-6 border-b border-gray-200 dark:border-gray-800"
      >
        <UInput
          v-model="searchQuery"
          :placeholder="t('search.placeholder')"
          icon="i-heroicons-magnifying-glass"
          class="flex-1"
        />
        <USelectMenu
          v-model="selectedCategory"
          :items="categoryItems"
          :placeholder="t('filter.category')"
          value-key="id"
          class="w-full @lg:w-48"
        >
          <template #leading>
            <UIcon name="i-heroicons-tag" />
          </template>
        </USelectMenu>
      </div>

      <!-- Loading State -->
      <div
        v-if="isInitialLoading || marketplace.isLoading.value"
        class="flex-1 flex flex-col items-center justify-center gap-3"
      >
        <UIcon
          name="i-heroicons-arrow-path"
          class="w-8 h-8 animate-spin text-gray-400"
        />
        <p class="text-sm text-gray-500">{{ t('loading') }}</p>
      </div>

      <!-- Extensions Grid -->
      <div
        v-else-if="extensionViewModels.length"
        class="flex-1 overflow-auto p-6"
      >
        <div class="grid grid-cols-1 @xl:grid-cols-2 gap-6">
          <HaexExtensionMarketplaceCard
            v-for="ext in extensionViewModels"
            :key="ext.id"
            :extension="ext"
            @install="onInstallFromMarketplace(ext)"
            @update="onUpdateExtension(ext)"
            @details="onShowExtensionDetails(ext)"
            @remove="onRemoveExtension(ext)"
          />
        </div>

        <!-- Pagination -->
        <div
          v-if="marketplace.extensionsTotal.value > 20"
          class="flex justify-center mt-6"
        >
          <UPagination
            v-model="currentPage"
            :total="marketplace.extensionsTotal.value"
            :items-per-page="20"
          />
        </div>
      </div>

      <!-- Empty State -->
      <div
        v-else
        class="flex flex-col items-center justify-center flex-1 text-center p-6"
      >
        <UIcon
          name="i-heroicons-puzzle-piece"
          class="w-16 h-16 text-gray-400 mb-4"
        />
        <h3 class="text-lg font-semibold text-gray-900 dark:text-white">
          {{ t('empty.title') }}
        </h3>
        <p class="text-gray-500 dark:text-gray-400 mt-2">
          {{ t('empty.description') }}
        </p>
      </div>

      <HaexExtensionDialogReinstall
        v-model:open="openOverwriteDialog"
        v-model:preview="installPreview"
        :mode="reinstallMode"
        :icon-url="currentMarketplaceExtension?.iconUrl"
        @confirm="confirmReinstallAsync"
      />

      <HaexExtensionDialogInstall
        v-model:open="showConfirmation"
        :preview="installPreview"
        :available-versions="currentMarketplaceExtension?.versions"
        :installed-version="currentMarketplaceExtension?.installedVersion"
        :icon-url="currentMarketplaceExtension?.iconUrl"
        @confirm="confirmInstallAsync"
      />

      <HaexExtensionDialogRemove
        v-model:open="showRemoveDialog"
        :extension="extensionToBeRemoved"
        @confirm="removeExtensionAsync"
      />

      <HaexExtensionDialogDetails
        v-model:open="showDetailsDialog"
        :extension="selectedExtensionForDetails"
        @install="onInstallFromMarketplace"
        @update="onUpdateExtension"
        @remove="onRemoveExtension"
      />
    </div>
  </HaexSystem>
</template>

<script setup lang="ts">
import type {
  IHaexSpaceExtension,
  IHaexSpaceExtensionManifest,
  MarketplaceExtensionViewModel,
} from '~/types/haexspace'
import { useMarketplace } from '@haex-space/marketplace-sdk/vue'
import { open } from '@tauri-apps/plugin-dialog'
import { invoke } from '@tauri-apps/api/core'
import type { ExtensionPreview } from '~~/src-tauri/bindings/ExtensionPreview'
import { isDesktop } from '~/utils/platform'

const props = defineProps<{
  isDragging?: boolean
  windowParams?: Record<string, unknown>
}>()

const { t } = useI18n()
const extensionStore = useExtensionsStore()

const showConfirmation = ref(false)
const openOverwriteDialog = ref(false)

const extension = reactive<{
  manifest: IHaexSpaceExtensionManifest | null | undefined
  path: string | null
}>({
  manifest: null,
  path: '',
})

const { add } = useToast()
const { addNotificationAsync } = useNotificationStore()

const preview = ref<ExtensionPreview>()

// Track installation source: 'file' or 'marketplace'
const installSource = ref<'file' | 'marketplace'>('file')

// Combined preview from either file or marketplace download
const installPreview = computed(() => {
  if (installSource.value === 'marketplace') {
    return extensionStore.preview
  }
  return preview.value
})

// Marketplace SDK
const marketplace = useMarketplace()

// State
const searchQuery = ref('')
const selectedCategory = ref<string | null>(null)
const currentPage = ref(1)
const isInitialLoading = ref(true)

// Debounced search
const debouncedSearch = refDebounced(searchQuery, 300)

// Category items for select menu
const categoryItems = computed(() => {
  const allCategory = { id: null, label: t('category.all') }
  const apiCategories = marketplace.categories.value.map((cat) => ({
    id: cat.slug,
    label: cat.name,
  }))
  return [allCategory, ...apiCategories]
})

// Transform API extensions to view models with installation status
const extensionViewModels = computed((): MarketplaceExtensionViewModel[] => {
  return marketplace.extensions.value.map((ext) => {
    // Find if this extension is installed locally by matching name
    const installedExt = extensionStore.availableExtensions.find(
      (installed) => installed.name === ext.name,
    )

    return {
      ...ext,
      isInstalled: !!installedExt,
      installedVersion: installedExt?.version,
      latestVersion: ext.versions?.[0]?.version,
    }
  })
})

// Load extensions from API
const loadExtensionsAsync = async () => {
  try {
    await marketplace.fetchExtensions({
      page: currentPage.value,
      limit: 20,
      category: selectedCategory.value || undefined,
      search: debouncedSearch.value || undefined,
      sort: 'downloads',
    })
  } catch (error) {
    console.error('Failed to load marketplace extensions:', error)
    add({ color: 'error', description: t('error.loadExtensions') })
  }
}

// Load categories from API
const loadCategoriesAsync = async () => {
  try {
    await marketplace.fetchCategories()
  } catch (error) {
    console.error('Failed to load categories:', error)
  }
}

// Watch for filter changes
watch([debouncedSearch, selectedCategory, currentPage], () => {
  loadExtensionsAsync()
})

// Reset page when filters change
watch([debouncedSearch, selectedCategory], () => {
  currentPage.value = 1
})

// Current extension being installed from marketplace
const currentMarketplaceExtension = ref<MarketplaceExtensionViewModel | null>(
  null,
)
const isDownloading = ref(false)

// Reinstall mode: 'update' preserves data, 'reinstall' deletes everything
const reinstallMode = ref<'update' | 'reinstall'>('reinstall')

// Install from marketplace
const onInstallFromMarketplace = async (ext: MarketplaceExtensionViewModel) => {
  currentMarketplaceExtension.value = ext
  installSource.value = 'marketplace'
  isDownloading.value = true

  try {
    // Get download URL from marketplace API
    const downloadInfo = await marketplace.getDownloadUrl(ext.slug)

    // Download and preview
    await extensionStore.downloadAndPreviewAsync(
      downloadInfo.downloadUrl,
      downloadInfo.bundleHash,
    )

    // Ensure extensions list is up-to-date before checking
    await extensionStore.loadExtensionsAsync()

    const previewManifest = extensionStore.preview?.manifest
    if (!previewManifest) {
      throw new Error('No preview manifest available')
    }

    // Check if extension files are actually installed locally (not just in DB from sync)
    const isLocallyInstalled = await extensionStore.isExtensionInstalledAsync({
      publicKey: previewManifest.publicKey,
      name: previewManifest.name,
      version: previewManifest.version,
    })

    if (isLocallyInstalled) {
      // Files exist locally - show reinstall dialog
      reinstallMode.value = 'reinstall'
      openOverwriteDialog.value = true
    } else {
      // Not locally installed (may exist in DB from sync) - show normal install dialog
      // The Rust backend handles the UPSERT case
      showConfirmation.value = true
    }
  } catch (error) {
    console.error('Failed to download extension:', error)
    add({ color: 'error', description: t('error.downloadExtension') })
    extensionStore.clearPendingInstall()
  } finally {
    isDownloading.value = false
  }
}

// Update extension from marketplace (preserves data)
const onUpdateExtension = async (ext: MarketplaceExtensionViewModel) => {
  currentMarketplaceExtension.value = ext
  installSource.value = 'marketplace'
  isDownloading.value = true

  try {
    // Get download URL from marketplace API
    const downloadInfo = await marketplace.getDownloadUrl(ext.slug)

    // Download and preview
    await extensionStore.downloadAndPreviewAsync(
      downloadInfo.downloadUrl,
      downloadInfo.bundleHash,
    )

    // Set mode to update (preserves data)
    reinstallMode.value = 'update'
    openOverwriteDialog.value = true
  } catch (error) {
    console.error('Failed to download extension:', error)
    add({ color: 'error', description: t('error.downloadExtension') })
    extensionStore.clearPendingInstall()
  } finally {
    isDownloading.value = false
  }
}

// Show extension details
const showDetailsDialog = ref(false)
const selectedExtensionForDetails = ref<MarketplaceExtensionViewModel | null>(
  null,
)

const onShowExtensionDetails = (ext: MarketplaceExtensionViewModel) => {
  selectedExtensionForDetails.value = ext
  showDetailsDialog.value = true
}

const onRemoveExtension = (ext: MarketplaceExtensionViewModel) => {
  // Find the installed extension by name
  const installedExt = extensionStore.availableExtensions.find(
    (installed) => installed.name === ext.name,
  )
  if (installedExt) {
    extensionToBeRemoved.value = installedExt
    showRemoveDialog.value = true
  }
}

const onSelectExtensionAsync = async () => {
  installSource.value = 'file'

  try {
    extension.path = await open({ directory: false, recursive: true })
    if (!extension.path) return

    preview.value = await extensionStore.previewManifestAsync(extension.path)

    if (!preview.value?.manifest) return

    // Check if extension files are actually installed locally (not just in DB from sync)
    const isLocallyInstalled = await extensionStore.isExtensionInstalledAsync({
      publicKey: preview.value.manifest.publicKey,
      name: preview.value.manifest.name,
      version: preview.value.manifest.version,
    })

    if (isLocallyInstalled) {
      // Files exist locally - show reinstall dialog
      openOverwriteDialog.value = true
    } else {
      // Not locally installed (may exist in DB from sync) - show normal install dialog
      // The Rust backend handles the UPSERT case
      showConfirmation.value = true
    }
  } catch (error) {
    add({ color: 'error', description: JSON.stringify(error) })
    await addNotificationAsync({ text: JSON.stringify(error), type: 'error' })
  }
}

// Unified install function that handles both file and marketplace sources
const confirmInstallAsync = async (
  createDesktopShortcut: boolean = false,
  selectedVersion: string | null = null,
) => {
  try {
    let installedExtensionId: string | undefined

    // If a different version was selected, download that version first
    if (
      installSource.value === 'marketplace' &&
      selectedVersion &&
      currentMarketplaceExtension.value &&
      selectedVersion !== extensionStore.preview?.manifest.version
    ) {
      // Download the selected version
      const downloadInfo = await marketplace.getDownloadUrl(
        currentMarketplaceExtension.value.slug,
        selectedVersion,
      )
      await extensionStore.downloadAndPreviewAsync(
        downloadInfo.downloadUrl,
        downloadInfo.bundleHash,
      )
    }

    const previewToUse =
      installSource.value === 'marketplace'
        ? extensionStore.preview
        : preview.value

    const previewManifest = previewToUse?.manifest

    if (installSource.value === 'marketplace') {
      // Check if extension exists in DB (e.g., from sync) but not locally installed
      const existingExt = previewManifest
        ? extensionStore.availableExtensions.find(
            (ext) =>
              ext.publicKey === previewManifest.publicKey &&
              ext.name === previewManifest.name,
          )
        : undefined

      if (existingExt) {
        // Extension exists in DB from sync - only install files
        console.log(`Extension exists in DB, installing files only`)
        installedExtensionId = await extensionStore.installFilesAsync(
          existingExt.id,
        )
      } else {
        // New extension - full installation (DB + files)
        installedExtensionId = await extensionStore.installPendingAsync(
          extensionStore.preview?.editablePermissions,
        )
      }
    } else {
      // Install from file - check same condition
      const existingExt = previewManifest
        ? extensionStore.availableExtensions.find(
            (ext) =>
              ext.publicKey === previewManifest.publicKey &&
              ext.name === previewManifest.name,
          )
        : undefined

      if (existingExt) {
        // Extension exists in DB from sync - only install files
        console.log(`Extension exists in DB, installing files only`)
        installedExtensionId = await extensionStore.installFilesAsync(
          existingExt.id,
        )
      } else {
        // New extension - full installation
        installedExtensionId = await extensionStore.installAsync(
          extension.path,
          preview.value?.editablePermissions,
        )
      }
    }

    await extensionStore.loadExtensionsAsync()

    // Trigger a sync pull to fetch existing extension data from server
    // This is important when reinstalling an extension - we want to pull
    // all historical data that was synced from other devices
    try {
      const syncOrchestratorStore = useSyncOrchestratorStore()
      const syncBackendsStore = useSyncBackendsStore()
      const enabledBackends = syncBackendsStore.enabledBackends

      if (enabledBackends.length > 0) {
        console.log('[Extension Install] Triggering sync pull to fetch extension data...')
        await Promise.allSettled(
          enabledBackends.map((backend) =>
            syncOrchestratorStore.pullFromBackendAsync(backend.id),
          ),
        )
        console.log('[Extension Install] Sync pull completed')
      }
    } catch (error) {
      // Don't fail installation if sync pull fails
      console.warn('[Extension Install] Sync pull failed:', error)
    }

    // Automatically add extension to internal HaexVault desktop
    if (installedExtensionId) {
      try {
        const desktopStore = useDesktopStore()
        const workspaceStore = useWorkspaceStore()

        // Ensure workspaces are loaded before adding desktop item
        if (!workspaceStore.currentWorkspace) {
          console.log('[Extension Install] No workspace loaded, loading workspaces...')
          await workspaceStore.loadWorkspacesAsync()
        }

        console.log('[Extension Install] Adding desktop item for extension:', installedExtensionId)
        console.log('[Extension Install] Current workspace:', workspaceStore.currentWorkspace?.id)

        await desktopStore.addDesktopItemAsync(
          'extension',
          installedExtensionId,
        )
        console.log('[Extension Install] Desktop item added successfully')
      } catch (error) {
        console.warn('Could not add extension to desktop:', error)
      }

      // Create native desktop shortcut if requested (only on desktop platforms)
      if (createDesktopShortcut && isDesktop()) {
        try {
          await createNativeDesktopShortcut(installedExtensionId)
        } catch (error) {
          console.warn('Could not create native desktop shortcut:', error)
          // Don't fail the installation, just show a warning
          add({
            color: 'warning',
            title: t('extension.shortcut.error.title'),
            description: t('extension.shortcut.error.text'),
          })
        }
      }
    }

    // Refresh marketplace list if we came from marketplace
    if (installSource.value === 'marketplace') {
      await loadExtensionsAsync()
    }

    const extName =
      installSource.value === 'marketplace'
        ? currentMarketplaceExtension.value?.name
        : extension.manifest?.name

    add({
      color: 'success',
      title: t('extension.success.title', { extension: extName }),
      description: t('extension.success.text'),
    })
    await addNotificationAsync({
      text: t('extension.success.text'),
      type: 'success',
      title: t('extension.success.title', { extension: extName }),
    })
  } catch (error) {
    console.error('Fehler confirmInstallAsync:', error)
    add({ color: 'error', description: JSON.stringify(error) })
    await addNotificationAsync({ text: JSON.stringify(error), type: 'error' })
  } finally {
    if (installSource.value === 'marketplace') {
      currentMarketplaceExtension.value = null
      extensionStore.clearPendingInstall()
    }
  }
}

// Create a native desktop shortcut using Tauri command
const createNativeDesktopShortcut = async (extensionId: string) => {
  await invoke('create_desktop_shortcut', {
    extensionId,
  })
}

// Unified reinstall function that handles both file and marketplace sources
const confirmReinstallAsync = async () => {
  try {
    const previewToUse =
      installSource.value === 'marketplace'
        ? extensionStore.preview
        : preview.value

    if (!previewToUse?.manifest) return

    // Find the installed extension to get its current version and ID
    const installedExt = extensionStore.availableExtensions.find(
      (ext) =>
        ext.publicKey === previewToUse.manifest.publicKey &&
        ext.name === previewToUse.manifest.name,
    )

    if (!installedExt) {
      // No installed extension found, just do a fresh install
      await confirmInstallAsync()
      return
    }

    // Save the extension ID before removing (needed for update mode)
    const existingExtensionId = installedExt.id
    const isUpdate = reinstallMode.value === 'update'

    // Remove old extension first
    // deleteData: true for reinstall (delete everything), false for update (preserve data)
    await extensionStore.removeExtensionAsync(
      installedExt.publicKey,
      installedExt.name,
      installedExt.version,
      !isUpdate, // deleteData: false for update, true for reinstall
    )

    if (isUpdate) {
      // Update mode: Install files only, keeping the existing DB entry and extension ID
      // This preserves the desktop icon reference
      await extensionStore.installFilesAsync(existingExtensionId)

      // Reload extensions list
      await extensionStore.loadExtensionsAsync()

      // Show success message
      const extName = previewToUse.manifest.name
      add({
        color: 'success',
        title: t('extension.success.title', { extension: extName }),
        description: t('extension.success.text'),
      })
      await addNotificationAsync({
        text: t('extension.success.text'),
        type: 'success',
        title: t('extension.success.title', { extension: extName }),
      })

      // Clear pending install data
      if (installSource.value === 'marketplace') {
        currentMarketplaceExtension.value = null
        extensionStore.clearPendingInstall()
      }
    } else {
      // Reinstall mode: Full fresh install (DB entry was deleted)
      await confirmInstallAsync()
    }
  } catch (error) {
    console.error('Fehler confirmReinstallAsync:', error)
    add({ color: 'error', description: JSON.stringify(error) })
    await addNotificationAsync({ text: JSON.stringify(error), type: 'error' })
  }
}

const extensionToBeRemoved = ref<IHaexSpaceExtension>()
const showRemoveDialog = ref(false)

// Handle window params (e.g., update action from settings)
const handleWindowParams = async () => {
  if (!props.windowParams) return

  const { action, extensionName } = props.windowParams as {
    action?: string
    extensionName?: string
  }

  if (action === 'update' && extensionName) {
    // Set search query to find the extension
    searchQuery.value = extensionName

    // Wait for search to complete
    await nextTick()
    await loadExtensionsAsync()

    // Find the extension in the results and trigger update
    const ext = extensionViewModels.value.find(
      (e) => e.name === extensionName,
    )
    if (ext && ext.isInstalled) {
      onUpdateExtension(ext)
    }
  }
}

// Load data on mount
onMounted(async () => {
  try {
    await Promise.all([
      extensionStore.loadExtensionsAsync(),
      loadCategoriesAsync(),
      loadExtensionsAsync(),
    ])

    // Handle any window params after initial load
    await handleWindowParams()
  } catch (error) {
    console.error('Failed to load data:', error)
    add({ color: 'error', description: t('error.loadExtensions') })
  } finally {
    isInitialLoading.value = false
  }
})

const removeExtensionAsync = async (deleteMode: 'device' | 'complete') => {
  if (!extensionToBeRemoved.value) {
    add({
      color: 'error',
      description: 'Erweiterung kann nicht gelöscht werden',
    })
    return
  }

  try {
    // Uninstall extension (handles dev/regular, removes desktop items, reloads installed list)
    await extensionStore.uninstallExtensionAsync(
      extensionToBeRemoved.value.id,
      deleteMode,
    )

    const successKey =
      deleteMode === 'complete'
        ? 'extension.remove.success.complete'
        : 'extension.remove.success.device'

    add({
      color: 'success',
      title: t(`${successKey}.title`, {
        extensionName: extensionToBeRemoved.value.name,
      }),
      description: t(`${successKey}.text`, {
        extensionName: extensionToBeRemoved.value.name,
      }),
    })
    await addNotificationAsync({
      text: t(`${successKey}.text`, {
        extensionName: extensionToBeRemoved.value.name,
      }),
      type: 'success',
      title: t(`${successKey}.title`, {
        extensionName: extensionToBeRemoved.value.name,
      }),
    })
  } catch (error) {
    add({
      color: 'error',
      title: t('extension.remove.error.title'),
      description: t('extension.remove.error.text', {
        error: JSON.stringify(error),
      }),
    })
    await addNotificationAsync({
      type: 'error',
      title: t('extension.remove.error.title'),
      text: t('extension.remove.error.text', { error: JSON.stringify(error) }),
    })
  }
}
</script>

<i18n lang="yaml">
de:
  title: Erweiterungen
  subtitle: Entdecke und installiere Erweiterungen für HaexSpace
  loading: Erweiterungen werden geladen...
  extension:
    installFromFile: Von Datei installieren
    add: Erweiterung hinzufügen
    success:
      title: '{extension} hinzugefügt'
      text: Die Erweiterung wurde erfolgreich hinzugefügt
    shortcut:
      error:
        title: Desktop-Verknüpfung fehlgeschlagen
        text: Die Erweiterung wurde installiert, aber die Desktop-Verknüpfung konnte nicht erstellt werden.
    remove:
      success:
        device:
          title: '{extensionName} deinstalliert'
          text: '{extensionName} wurde von diesem Gerät entfernt. Die Daten bleiben erhalten.'
        complete:
          title: '{extensionName} gelöscht'
          text: '{extensionName} und alle zugehörigen Daten wurden dauerhaft gelöscht.'
      error:
        text: "Erweiterung {extensionName} konnte nicht entfernt werden. \n {error}"
        title: 'Fehler beim Entfernen von {extensionName}'
    marketplace:
      comingSoon: Marketplace-Installation kommt bald!
  category:
    all: Alle Kategorien
  search:
    placeholder: Erweiterungen durchsuchen...
  filter:
    category: Kategorie auswählen
  empty:
    title: Keine Erweiterungen gefunden
    description: Versuche einen anderen Suchbegriff oder eine andere Kategorie
  error:
    loadExtensions: Erweiterungen konnten nicht geladen werden
    downloadExtension: Erweiterung konnte nicht heruntergeladen werden

en:
  title: Extensions
  subtitle: Discover and install extensions for HaexSpace
  loading: Loading extensions...
  extension:
    installFromFile: Install from file
    add: Add Extension
    success:
      title: '{extension} added'
      text: Extension was added successfully
    shortcut:
      error:
        title: Desktop shortcut failed
        text: The extension was installed, but the desktop shortcut could not be created.
    remove:
      success:
        device:
          title: '{extensionName} uninstalled'
          text: '{extensionName} was removed from this device. Data has been preserved.'
        complete:
          title: '{extensionName} deleted'
          text: '{extensionName} and all associated data have been permanently deleted.'
      error:
        text: "Extension {extensionName} couldn't be removed. \n {error}"
        title: 'Exception during uninstall {extensionName}'
    marketplace:
      comingSoon: Marketplace installation coming soon!
  category:
    all: All Categories
  search:
    placeholder: Search extensions...
  filter:
    category: Select category
  empty:
    title: No extensions found
    description: Try a different search term or category
  error:
    loadExtensions: Failed to load extensions
    downloadExtension: Failed to download extension
</i18n>
