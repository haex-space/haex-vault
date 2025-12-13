import { useMarketplace } from '@haex-space/marketplace-sdk/vue'
import type { ExtensionPreview } from '~~/src-tauri/bindings/ExtensionPreview'
import type { IHaexSpaceExtension } from '~/types/haexspace'

/**
 * Composable for handling extension updates from the marketplace.
 * Provides shared logic for downloading, previewing, and confirming updates.
 */
export function useExtensionUpdate() {
  const extensionsStore = useExtensionsStore()
  const marketplace = useMarketplace()
  const { add } = useToast()

  // State
  const isDownloading = ref(false)
  const updateDialogOpen = ref(false)
  const updatePreview = ref<ExtensionPreview | null>(null)
  const currentExtension = ref<IHaexSpaceExtension | null>(null)

  /**
   * Find extension in marketplace by name and return its slug
   */
  const findMarketplaceExtensionAsync = async (extensionName: string) => {
    await marketplace.fetchExtensions({ search: extensionName, limit: 10 })
    return marketplace.extensions.value.find((ext) => ext.name === extensionName)
  }

  /**
   * Download extension from marketplace by slug and prepare for update.
   * Opens the update dialog on success.
   * @param slug - The marketplace slug for the extension
   */
  const downloadBySlugForUpdateAsync = async (slug: string) => {
    if (isDownloading.value) return false

    isDownloading.value = true

    try {
      // Get download URL from marketplace API
      const downloadInfo = await marketplace.getDownloadUrl(slug)

      // Download and preview
      await extensionsStore.downloadAndPreviewAsync(
        downloadInfo.downloadUrl,
        downloadInfo.bundleHash,
      )

      // Set the preview for the dialog
      updatePreview.value = extensionsStore.preview || null

      // Show update dialog
      updateDialogOpen.value = true

      return true
    } catch (error) {
      console.error('Failed to download extension for update:', error)
      add({ description: 'Failed to download update', color: 'error' })
      extensionsStore.clearPendingInstall()
      return false
    } finally {
      isDownloading.value = false
    }
  }

  /**
   * Download extension from marketplace and prepare for update.
   * Searches for the extension by name first.
   * Opens the update dialog on success.
   * @param extension - The installed extension to update
   */
  const downloadForUpdateAsync = async (extension: IHaexSpaceExtension) => {
    if (isDownloading.value) return false

    currentExtension.value = extension

    // Find extension in marketplace
    const marketplaceExt = await findMarketplaceExtensionAsync(extension.name)

    if (!marketplaceExt) {
      add({ description: 'Extension not found in marketplace', color: 'error' })
      return false
    }

    return downloadBySlugForUpdateAsync(marketplaceExt.slug)
  }

  /**
   * Confirm and perform the update.
   * Removes old extension files (keeping data) and installs new files.
   */
  const confirmUpdateAsync = async (): Promise<boolean> => {
    try {
      if (!updatePreview.value?.manifest) {
        add({ description: 'No update preview available', color: 'error' })
        return false
      }

      // Find the installed extension to get its ID
      const installedExt = extensionsStore.availableExtensions.find(
        (ext) =>
          ext.publicKey === updatePreview.value!.manifest.publicKey &&
          ext.name === updatePreview.value!.manifest.name,
      )

      if (!installedExt) {
        add({ description: 'Extension not found', color: 'error' })
        return false
      }

      const existingExtensionId = installedExt.id

      // Remove old extension files but keep data (update mode)
      await extensionsStore.removeExtensionAsync(
        installedExt.publicKey,
        installedExt.name,
        installedExt.version,
        false, // deleteData: false for update (preserve data)
      )

      // Install new files, keeping the existing DB entry and extension ID
      await extensionsStore.installFilesAsync(existingExtensionId)

      // Reload extensions list
      await extensionsStore.loadExtensionsAsync()

      add({ description: 'Extension updated successfully', color: 'success' })

      return true
    } catch (error) {
      console.error('Failed to update extension:', error)
      add({ description: 'Failed to update extension', color: 'error' })
      return false
    } finally {
      cleanup()
    }
  }

  /**
   * Clean up state after update or cancellation
   */
  const cleanup = () => {
    extensionsStore.clearPendingInstall()
    updateDialogOpen.value = false
    updatePreview.value = null
    currentExtension.value = null
  }

  return {
    // State
    isDownloading: readonly(isDownloading),
    updateDialogOpen,
    updatePreview,
    currentExtension: readonly(currentExtension),

    // Methods
    downloadForUpdateAsync,
    downloadBySlugForUpdateAsync,
    confirmUpdateAsync,
    cleanup,
  }
}
