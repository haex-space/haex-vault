/**
 * Extension Ready Store
 *
 * Tracks the ready state of extensions across all platforms.
 * An extension is considered "ready" after it has completed its initialization,
 * including database migrations and setup hooks.
 *
 * This store is used for:
 * - ExternalBridge (Desktop): Waiting for extension to be ready before routing requests
 * - Extension-to-Extension communication: One extension waiting for another to be ready
 */

import { invoke } from '@tauri-apps/api/core'
import { isDesktop } from '~/utils/platform'

interface ExtensionReadyState {
  isReady: boolean
  readyAt: Date | null
}

export const useExtensionReadyStore = defineStore('extensionReady', () => {
  // Map of extensionId -> ready state
  const readyStates = ref<Map<string, ExtensionReadyState>>(new Map())

  // Callbacks waiting for extensions to become ready
  // Map of extensionId -> array of resolve functions
  const waitingCallbacks = new Map<string, Array<() => void>>()

  /**
   * Mark an extension as ready.
   * This is called after the extension has completed its initialization.
   */
  const signalReady = async (extensionId: string) => {
    console.log(`[ExtensionReady] Extension ${extensionId} signaled ready`)

    // Update local state
    readyStates.value.set(extensionId, {
      isReady: true,
      readyAt: new Date(),
    })

    // Notify any waiting callbacks
    const callbacks = waitingCallbacks.get(extensionId)
    if (callbacks) {
      callbacks.forEach(resolve => resolve())
      waitingCallbacks.delete(extensionId)
    }

    // On Desktop, also notify the backend ExternalBridge
    if (isDesktop()) {
      try {
        await invoke('extension_signal_ready', { extensionId })
        console.log(`[ExtensionReady] Notified backend for extension ${extensionId}`)
      }
      catch (error) {
        // Don't fail if backend notification fails
        console.warn(`[ExtensionReady] Failed to notify backend for ${extensionId}:`, error)
      }
    }
  }

  /**
   * Check if an extension is ready.
   */
  const isReady = (extensionId: string): boolean => {
    return readyStates.value.get(extensionId)?.isReady ?? false
  }

  /**
   * Wait for an extension to become ready.
   * Returns immediately if already ready, otherwise waits with optional timeout.
   */
  const waitForReady = (extensionId: string, timeoutMs?: number): Promise<boolean> => {
    // Already ready
    if (isReady(extensionId)) {
      return Promise.resolve(true)
    }

    return new Promise((resolve) => {
      // Add to waiting callbacks
      if (!waitingCallbacks.has(extensionId)) {
        waitingCallbacks.set(extensionId, [])
      }
      waitingCallbacks.get(extensionId)!.push(() => resolve(true))

      // Set timeout if specified
      if (timeoutMs !== undefined) {
        setTimeout(() => {
          const callbacks = waitingCallbacks.get(extensionId)
          if (callbacks) {
            const index = callbacks.findIndex(cb => cb === resolve)
            if (index !== -1) {
              callbacks.splice(index, 1)
              resolve(false) // Timeout reached
            }
          }
        }, timeoutMs)
      }
    })
  }

  /**
   * Reset the ready state for an extension.
   * Called when an extension is unloaded or restarted.
   */
  const resetReady = (extensionId: string) => {
    readyStates.value.delete(extensionId)
    waitingCallbacks.delete(extensionId)
  }

  /**
   * Reset all ready states.
   * Called when the vault is closed.
   */
  const resetAll = () => {
    readyStates.value.clear()
    waitingCallbacks.clear()
  }

  return {
    readyStates,
    signalReady,
    isReady,
    waitForReady,
    resetReady,
    resetAll,
  }
})
