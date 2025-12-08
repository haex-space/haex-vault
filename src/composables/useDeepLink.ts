/**
 * Deep-Link Handler for HaexVault
 *
 * Handles deep-link URLs in the format: haexvault://extension/{extension-id}
 * This allows launching HaexVault and opening a specific extension directly.
 *
 * Important: The vault must be unlocked before extensions can be accessed.
 * If the vault is locked, the extension ID is stored and processed after unlock.
 */

import { getCurrent, onOpenUrl } from '@tauri-apps/plugin-deep-link'
import { listen } from '@tauri-apps/api/event'
import { isDesktop } from '~/utils/platform'

// Store pending deep-link outside of composable for persistence across component mounts
const pendingExtensionId = ref<string | null>(null)

export const useDeepLink = () => {
  const windowManager = useWindowManagerStore()
  const vaultStore = useVaultStore()

  /**
   * Parse a deep-link URL and extract the extension ID
   * Format: haexvault://extension/{extension-id}
   */
  const parseDeepLinkUrl = (url: string): string | null => {
    const match = url.match(/haexvault:\/\/extension\/(.+)/)
    return match?.[1] ?? null
  }

  /**
   * Check if a vault is currently open (unlocked)
   */
  const isVaultOpen = computed(() => {
    return !!vaultStore.currentVault
  })

  /**
   * Handle a deep-link URL
   * If the vault is locked, stores the extension ID for later processing
   */
  const handleDeepLink = async (url: string) => {
    console.log('[DeepLink] Received URL:', url)

    const extensionId = parseDeepLinkUrl(url)
    if (!extensionId) {
      console.warn('[DeepLink] Invalid deep-link URL format:', url)
      return
    }

    console.log('[DeepLink] Parsed extension ID:', extensionId)

    // Check if vault is open
    if (!isVaultOpen.value) {
      console.log('[DeepLink] Vault is locked, storing extension ID for later:', extensionId)
      pendingExtensionId.value = extensionId
      return
    }

    // Vault is open, open the extension window
    await openExtensionWindow(extensionId)
  }

  /**
   * Open an extension window by its ID
   */
  const openExtensionWindow = async (extensionId: string) => {
    try {
      console.log('[DeepLink] Opening extension window:', extensionId)

      await windowManager.openWindowAsync({
        type: 'extension',
        sourceId: extensionId,
      })

      console.log('[DeepLink] Extension window opened successfully')
    } catch (error) {
      console.error('[DeepLink] Failed to open extension window:', error)
    }
  }

  /**
   * Process any pending deep-link after vault unlock
   */
  const processPendingDeepLink = async () => {
    if (pendingExtensionId.value) {
      console.log('[DeepLink] Processing pending extension:', pendingExtensionId.value)

      const extensionId = pendingExtensionId.value
      pendingExtensionId.value = null

      await openExtensionWindow(extensionId)
    }
  }

  /**
   * Initialize the deep-link handler
   * Should be called once when the app starts
   */
  const init = async () => {
    if (!isDesktop()) {
      console.log('[DeepLink] Not on desktop, skipping initialization')
      return
    }

    console.log('[DeepLink] Initializing deep-link handler...')

    try {
      // Check if app was launched with a deep-link URL
      const startUrls = await getCurrent()
      if (startUrls && startUrls.length > 0) {
        console.log('[DeepLink] App started with URLs:', startUrls)
        for (const url of startUrls) {
          await handleDeepLink(url)
        }
      }

      // Listen for deep-links when app is already running
      await onOpenUrl((urls) => {
        console.log('[DeepLink] Received URLs while running:', urls)
        for (const url of urls) {
          handleDeepLink(url)
        }
      })

      // Listen for deep-link events from single-instance plugin
      // (when another instance tries to start with a deep-link URL)
      await listen<string>('deep-link-received', (event) => {
        console.log('[DeepLink] Received from single-instance:', event.payload)
        handleDeepLink(event.payload)
      })

      console.log('[DeepLink] Deep-link handler initialized')
    } catch (error) {
      console.error('[DeepLink] Failed to initialize:', error)
    }

    // Watch for vault unlock to process pending deep-links
    watch(isVaultOpen, (isOpen) => {
      if (isOpen) {
        console.log('[DeepLink] Vault unlocked, checking for pending deep-links')
        processPendingDeepLink()
      }
    })
  }

  return {
    init,
    handleDeepLink,
    processPendingDeepLink,
    pendingExtensionId: readonly(pendingExtensionId),
    isVaultOpen,
  }
}
