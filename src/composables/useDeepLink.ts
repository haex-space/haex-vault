import { SettingsCategory } from '~/config/settingsCategories'
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
import { isInviteLink, parseInviteTokenLink } from '~/utils/inviteLink'

// Store pending deep-link outside of composable for persistence across component mounts
const pendingExtensionId = ref<string | null>(null)
const pendingInviteLink = ref<string | null>(null)
let initialized = false

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
   * Supports: haexvault://extension/{id} and haexvault://invite/{base58}
   * If the vault is locked, stores the action for later processing
   */
  const handleDeepLink = async (url: string) => {
    if (isInviteLink(url)) {
      if (!isVaultOpen.value) {
        pendingInviteLink.value = url
        return
      }
      await handleInviteLink(url)
      return
    }

    const extensionId = parseDeepLinkUrl(url)
    if (!extensionId) {
      console.warn('[DeepLink] Invalid deep-link URL format:', url)
      return
    }

    if (!isVaultOpen.value) {
      pendingExtensionId.value = extensionId
      return
    }

    await openExtensionWindow(extensionId)
  }

  /**
   * Open an extension window by its ID
   */
  const openExtensionWindow = async (extensionId: string) => {
    try {
      await windowManager.openWindowAsync({
        type: 'extension',
        sourceId: extensionId,
      })
    } catch (error) {
      console.error('[DeepLink] Failed to open extension window:', error)
    }
  }

  /**
   * Handle an invite link — parse token and open claim flow
   */
  const handleInviteLink = async (url: string) => {
    const tokenLink = parseInviteTokenLink(url)
    if (tokenLink) {
      // Token-based invite: open settings with token params for claim flow
      try {
        await windowManager.openWindowAsync({
          type: 'system',
          sourceId: 'settings',
          params: {
            category: SettingsCategory.Spaces,
            inviteToken: tokenLink,
          },
        })
      } catch (error) {
        console.error('[DeepLink] Failed to open invite claim dialog:', error)
      }
    }
  }

  /**
   * Process any pending deep-link after vault unlock
   */
  const processPendingDeepLink = async () => {
    if (pendingInviteLink.value) {
      const link = pendingInviteLink.value
      pendingInviteLink.value = null
      await handleInviteLink(link)
      return
    }

    if (pendingExtensionId.value) {
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
    if (initialized || !isDesktop()) {
      return
    }
    initialized = true

    try {
      // Check if app was launched with a deep-link URL
      const startUrls = await getCurrent()
      if (startUrls && startUrls.length > 0) {
        for (const url of startUrls) {
          await handleDeepLink(url)
        }
      }

      // Listen for deep-links when app is already running
      await onOpenUrl((urls) => {
        for (const url of urls) {
          handleDeepLink(url)
        }
      })

      // Listen for deep-link events from single-instance plugin
      // (when another instance tries to start with a deep-link URL)
      await listen<string>('deep-link-received', (event) => {
        handleDeepLink(event.payload)
      })
    } catch (error) {
      console.error('[DeepLink] Failed to initialize:', error)
    }

    // Watch for vault unlock to process pending deep-links
    watch(isVaultOpen, (isOpen) => {
      if (isOpen) {
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
