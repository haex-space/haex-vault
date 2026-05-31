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
// If an auth request arrives while another modal (e.g. AddContact) is open,
// or while the auth dialog is already showing a different client, we cannot
// pop it right away: Reka UI's DismissableLayer pushes a new focus trap
// onto `body` and the new dialog ends up z-stacked behind the active modal
// AND inerts it at the same time → UI deadlocks. We therefore queue
// pending requests keyed by `clientId` and drain them one by one once the
// DOM is free. A Map keeps concurrent requests from distinct clients
// distinguishable — a plain ref<PendingAuthorization | null> would let a
// later request silently overwrite an earlier one, and the backend's
// `already_pending` dedup would then suppress the re-emit, stranding the
// first client forever.
const queuedAuth = new Map<string, PendingAuthorization>()
let dialogObserver: MutationObserver | null = null

/**
 * Reka UI sets `role="dialog"` + `data-state="open"` on every visible
 * dialog content (see reka-ui/Dialog/DialogContentImpl.vue). We use that
 * to detect whether *any* modal is currently shown. Returns false when
 * called server-side.
 */
function hasOtherOpenDialog(): boolean {
  if (typeof document === 'undefined') return false
  return document.querySelector('[role="dialog"][data-state="open"]') !== null
}

/** Pull the oldest queued request (FIFO via Map insertion order). */
function dequeueNext(): PendingAuthorization | null {
  const next = queuedAuth.values().next()
  if (next.done) return null
  queuedAuth.delete(next.value.clientId)
  return next.value
}

/**
 * If nothing is blocking the dialog right now, present the next queued
 * request. Called from the MutationObserver when DOM state changes.
 */
function tryDrainQueue() {
  if (queuedAuth.size === 0) {
    stopWatchingDom()
    return
  }
  if (isOpen.value) return
  if (hasOtherOpenDialog()) return
  const next = dequeueNext()
  if (!next) {
    stopWatchingDom()
    return
  }
  if (queuedAuth.size === 0) stopWatchingDom()
  void presentDialogFromQueue(next)
}

/**
 * Watch the DOM for the `data-state` attribute on dialogs to flip. As soon
 * as no other dialog is open, drain the queue by showing the next auth
 * request. The observer is shared across `useExternalAuth()` callers and
 * disconnects itself once the queue is empty.
 */
function startWatchingDom() {
  if (dialogObserver || typeof document === 'undefined') return
  dialogObserver = new MutationObserver(() => tryDrainQueue())
  dialogObserver.observe(document.body, {
    subtree: true,
    attributes: true,
    attributeFilter: ['data-state'],
    childList: true,
  })
}

function stopWatchingDom() {
  dialogObserver?.disconnect()
  dialogObserver = null
}

/**
 * Module-scoped flush helper — needed because `startWatchingDom` lives
 * outside the composable closure and can't reach the inner
 * `presentAuthDialog`. Mirrors the same focus + state-set sequence.
 */
async function presentDialogFromQueue(auth: PendingAuthorization) {
  try {
    await invoke('focus_main_window')
  } catch (error) {
    console.warn('[ExternalAuth] Failed to focus window:', error)
  }
  pendingAuth.value = auth
  isOpen.value = true
}

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

    try {
      // Listen for authorization requests from the Tauri backend.
      // Backend emits via emit_to("main", …); Tauri v2 deliver only matches
      // here when the listener carries an AnyLabel target (passing the
      // string 'main' is the shorthand). A bare listen() with default
      // target=Any is dropped on the floor in production builds — that
      // was the regression behind the haex-pass and auto-start outages.
      await listen<PendingAuthorization>(
        EXTERNAL_EVENTS.AUTHORIZATION_REQUEST,
        (event) => {
          showAuthorizationPrompt(event.payload)
        },
        { target: 'main' },
      )

      initialized.value = true
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

    // Dedup against the visible dialog and the queue so a reconnect-looping
    // client cannot flood us. The backend also dedups in
    // pending_authorizations, this is the belt-and-braces guard for events
    // that slipped through (e.g. across a backend restart that cleared
    // pending state).
    if (isOpen.value && pendingAuth.value?.clientId === auth.clientId) return
    if (queuedAuth.has(auth.clientId)) return

    // If our own dialog is already showing a different client, or another
    // modal (AddContact, file preview, …) is open, do NOT pop the auth
    // dialog now: Reka UI would z-stack it behind the active modal and
    // globally inert the page, leaving the user stuck. Queue and wait for
    // the DOM to free up.
    if (isOpen.value || hasOtherOpenDialog()) {
      queuedAuth.set(auth.clientId, auth)
      startWatchingDom()
      return
    }

    await presentDialogFromQueue(auth)
  }

  /**
   * Handle user decision from the dialog
   */
  async function handleDecision(decision: ExternalAuthDecision, extensionIds?: string[], remember = false) {
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
              await invoke(TAURI_COMMANDS.externalBridge.clientAllow, {
                clientId: pendingAuth.value.clientId,
                clientName: pendingAuth.value.clientName,
                publicKey: pendingAuth.value.publicKey,
                extensionId,
                remember,
              })
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
    if (!isDesktop()) return []
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
    } catch (error) {
      console.error('[ExternalAuth] Failed to revoke client:', error)
      throw error
    }
  }

  /**
   * Get all blocked clients
   */
  async function getBlockedClients(): Promise<BlockedClient[]> {
    if (!isDesktop()) return []
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
    if (!isDesktop()) return []
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
    if (!isDesktop()) return []
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
