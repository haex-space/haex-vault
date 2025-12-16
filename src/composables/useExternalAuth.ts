import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { isDesktop } from '~/utils/platform'
import {
  TAURI_COMMANDS,
  EXTERNAL_EVENTS,
  type PendingAuthorization,
  type AuthorizedClient,
  type BlockedClient,
  type SessionAuthorization,
  type ExternalAuthDecision,
} from '@haex-space/vault-sdk'

// Global state for the authorization prompt
const isOpen = ref(false)
const pendingAuth = ref<PendingAuthorization | null>(null)
const initialized = ref(false)

/**
 * Composable for managing external client authorization prompts
 *
 * When a browser extension, CLI tool, or other external client connects
 * for the first time, this composable shows a dialog asking the user
 * to approve or deny the connection.
 */
export function useExternalAuth() {
  const vaultStore = useVaultStore()

  /**
   * Initialize the external auth event listeners
   * Should be called once when the app starts (desktop only)
   */
  async function init() {
    if (!isDesktop() || initialized.value) {
      return
    }

    console.log('[ExternalAuth] Initializing...')

    try {
      // Listen for authorization requests from the Tauri backend
      await listen<PendingAuthorization>(EXTERNAL_EVENTS.AUTHORIZATION_REQUEST, (event) => {
        console.log('[ExternalAuth] Received authorization request:', event.payload)
        showAuthorizationPrompt(event.payload)
      })

      initialized.value = true
      console.log('[ExternalAuth] Initialized')
    } catch (error) {
      console.error('[ExternalAuth] Failed to initialize:', error)
    }
  }

  /**
   * Show the authorization prompt dialog
   * Only shows if a vault is currently open
   */
  async function showAuthorizationPrompt(auth: PendingAuthorization) {
    // Don't show dialog if no vault is open - just ignore the request
    // The client will wait for a response and eventually timeout
    if (!vaultStore.currentVault) {
      console.warn('[ExternalAuth] Ignoring authorization request: no vault open')
      return
    }

    // Bring window to foreground so user notices the authorization request
    // Uses GTK present() on Linux for proper window focusing
    try {
      await invoke('focus_main_window')
      console.log('[ExternalAuth] Window focused')
    } catch (error) {
      console.warn('[ExternalAuth] Failed to focus window:', error)
    }

    pendingAuth.value = auth
    isOpen.value = true
  }

  /**
   * Handle user decision from the dialog
   */
  async function handleDecision(decision: ExternalAuthDecision, extensionIds?: string[], remember = false) {
    if (!pendingAuth.value) {
      return
    }

    try {
      switch (decision) {
        case 'allow':
          if (extensionIds && extensionIds.length > 0) {
            // Allow access for each selected extension
            for (const extensionId of extensionIds) {
              await invoke(TAURI_COMMANDS.external.clientAllow, {
                clientId: pendingAuth.value.clientId,
                clientName: pendingAuth.value.clientName,
                publicKey: pendingAuth.value.publicKey,
                extensionId,
                remember,
              })
            }
            if (remember) {
              console.log('[ExternalAuth] Authorization permanently approved for extensions:', extensionIds)
            } else {
              console.log('[ExternalAuth] Authorization allowed once for extensions:', extensionIds)
            }
          }
          break

        case 'deny':
          // Block this client (permanently if remember is true)
          await invoke(TAURI_COMMANDS.external.clientBlock, {
            clientId: pendingAuth.value.clientId,
            clientName: pendingAuth.value.clientName,
            publicKey: pendingAuth.value.publicKey,
            remember,
          })
          if (remember) {
            console.log('[ExternalAuth] Client permanently blocked:', pendingAuth.value.clientId)
          } else {
            console.log('[ExternalAuth] Request denied:', pendingAuth.value.clientId)
          }
          break
      }
    } catch (error) {
      console.error('[ExternalAuth] Failed to process decision:', error)
    }

    // Close dialog
    isOpen.value = false
    pendingAuth.value = null
  }

  /**
   * Cancel the prompt (denies the current request without blocking the client)
   */
  async function cancelPrompt() {
    if (!pendingAuth.value) {
      return
    }

    try {
      await invoke(TAURI_COMMANDS.external.clientBlock, {
        clientId: pendingAuth.value.clientId,
        clientName: pendingAuth.value.clientName,
        publicKey: pendingAuth.value.publicKey,
        remember: false,
      })
      console.log('[ExternalAuth] Request cancelled')
    } catch (error) {
      console.error('[ExternalAuth] Failed to cancel request:', error)
    }

    isOpen.value = false
    pendingAuth.value = null
  }

  /**
   * Get all authorized clients
   */
  async function getAuthorizedClients(): Promise<AuthorizedClient[]> {
    try {
      return await invoke<AuthorizedClient[]>(TAURI_COMMANDS.external.getAuthorizedClients)
    } catch (error) {
      console.error('[ExternalAuth] Failed to get authorized clients:', error)
      return []
    }
  }

  /**
   * Revoke authorization for a client
   */
  async function revokeClient(clientId: string): Promise<void> {
    try {
      await invoke(TAURI_COMMANDS.external.revokeClient, { clientId })
      console.log('[ExternalAuth] Client revoked:', clientId)
    } catch (error) {
      console.error('[ExternalAuth] Failed to revoke client:', error)
      throw error
    }
  }

  /**
   * Get all blocked clients
   */
  async function getBlockedClients(): Promise<BlockedClient[]> {
    try {
      return await invoke<BlockedClient[]>(TAURI_COMMANDS.external.getBlockedClients)
    } catch (error) {
      console.error('[ExternalAuth] Failed to get blocked clients:', error)
      return []
    }
  }

  /**
   * Unblock a client
   */
  async function unblockClient(clientId: string): Promise<void> {
    try {
      await invoke(TAURI_COMMANDS.external.unblockClient, { clientId })
      console.log('[ExternalAuth] Client unblocked:', clientId)
    } catch (error) {
      console.error('[ExternalAuth] Failed to unblock client:', error)
      throw error
    }
  }

  /**
   * Get all session-based authorizations (for "allow once")
   * These are cleared when haex-vault restarts
   */
  async function getSessionAuthorizations(): Promise<SessionAuthorization[]> {
    try {
      return await invoke<SessionAuthorization[]>(TAURI_COMMANDS.external.getSessionAuthorizations)
    } catch (error) {
      console.error('[ExternalAuth] Failed to get session authorizations:', error)
      return []
    }
  }

  /**
   * Revoke a session authorization
   */
  async function revokeSessionAuthorization(clientId: string): Promise<void> {
    try {
      await invoke(TAURI_COMMANDS.external.revokeSessionAuthorization, { clientId })
      console.log('[ExternalAuth] Session authorization revoked:', clientId)
    } catch (error) {
      console.error('[ExternalAuth] Failed to revoke session authorization:', error)
      throw error
    }
  }

  return {
    // State
    isOpen: readonly(isOpen),
    pendingAuth: readonly(pendingAuth),

    // Methods
    init,
    handleDecision,
    cancelPrompt,
    getAuthorizedClients,
    revokeClient,
    getBlockedClients,
    unblockClient,
    getSessionAuthorizations,
    revokeSessionAuthorization,
  }
}
