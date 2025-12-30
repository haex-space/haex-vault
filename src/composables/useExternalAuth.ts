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
import type { SessionBlockedClient } from '~~/src-tauri/bindings/SessionBlockedClient'

// Global state for the authorization prompt
const isOpen = ref(false)
const pendingAuth = ref<PendingAuthorization | null>(null)
const initialized = ref(false)
// Counter that increments when a decision is made (for reactive updates)
const decisionCounter = ref(0)

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
    console.log('[ExternalAuth] handleDecision called:', { decision, extensionIds, remember, pendingAuth: pendingAuth.value })

    if (!pendingAuth.value) {
      console.warn('[ExternalAuth] No pending auth, returning early')
      return
    }

    try {
      switch (decision) {
        case 'allow':
          if (extensionIds && extensionIds.length > 0) {
            // Allow access for each selected extension
            for (const extensionId of extensionIds) {
              console.log('[ExternalAuth] Calling clientAllow for extension:', extensionId)
              await invoke(TAURI_COMMANDS.externalBridge.clientAllow, {
                clientId: pendingAuth.value.clientId,
                clientName: pendingAuth.value.clientName,
                publicKey: pendingAuth.value.publicKey,
                extensionId,
                remember,
              })
              console.log('[ExternalAuth] clientAllow succeeded for extension:', extensionId)
            }
            if (remember) {
              console.log('[ExternalAuth] Authorization permanently approved for extensions:', extensionIds)
            } else {
              console.log('[ExternalAuth] Authorization allowed once for extensions:', extensionIds)
            }
          } else {
            console.warn('[ExternalAuth] No extensionIds provided for allow decision')
          }
          break

        case 'deny':
          // Block this client (permanently if remember is true)
          await invoke(TAURI_COMMANDS.externalBridge.clientBlock, {
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

    // Close dialog and notify listeners
    isOpen.value = false
    pendingAuth.value = null
    decisionCounter.value++
  }

  /**
   * Cancel the prompt (denies the current request without blocking the client)
   */
  async function cancelPrompt() {
    if (!pendingAuth.value) {
      return
    }

    try {
      await invoke(TAURI_COMMANDS.externalBridge.clientBlock, {
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
    decisionCounter.value++
  }

  /**
   * Get all authorized clients
   */
  async function getAuthorizedClients(): Promise<AuthorizedClient[]> {
    try {
      return await invoke<AuthorizedClient[]>(TAURI_COMMANDS.externalBridge.getAuthorizedClients)
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
      await invoke(TAURI_COMMANDS.externalBridge.revokeClient, { clientId })
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
      return await invoke<BlockedClient[]>(TAURI_COMMANDS.externalBridge.getBlockedClients)
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
      await invoke(TAURI_COMMANDS.externalBridge.unblockClient, { clientId })
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
      return await invoke<SessionAuthorization[]>(TAURI_COMMANDS.externalBridge.getSessionAuthorizations)
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
      await invoke(TAURI_COMMANDS.externalBridge.revokeSessionAuthorization, { clientId })
      console.log('[ExternalAuth] Session authorization revoked:', clientId)
    } catch (error) {
      console.error('[ExternalAuth] Failed to revoke session authorization:', error)
      throw error
    }
  }

  /**
   * Get all session-blocked clients (for "deny once")
   * These are cleared when haex-vault restarts
   */
  async function getSessionBlockedClients(): Promise<SessionBlockedClient[]> {
    try {
      return await invoke<SessionBlockedClient[]>('external_bridge_get_session_blocked_clients')
    } catch (error) {
      console.error('[ExternalAuth] Failed to get session blocked clients:', error)
      return []
    }
  }

  /**
   * Unblock a session-blocked client
   */
  async function unblockSessionClient(clientId: string): Promise<void> {
    try {
      await invoke('external_bridge_unblock_session_client', { clientId })
      console.log('[ExternalAuth] Session blocked client unblocked:', clientId)
    } catch (error) {
      console.error('[ExternalAuth] Failed to unblock session client:', error)
      throw error
    }
  }

  return {
    // State
    isOpen: readonly(isOpen),
    pendingAuth: readonly(pendingAuth),
    decisionCounter: readonly(decisionCounter),

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
    getSessionBlockedClients,
    unblockSessionClient,
  }
}
