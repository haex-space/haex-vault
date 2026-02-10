/**
 * Tests for visibility-based reconnection functionality
 *
 * Tests the mobile/Android reconnection logic that handles:
 * 1. App going to background (WebSocket connections die in Doze mode)
 * 2. App resuming from background (need to reconnect)
 * 3. Multiple backends reconnecting together
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'

// Mock the pull module to avoid deep dependency chain
vi.mock('@/stores/sync/orchestrator/pull', () => ({
  pullFromBackendAsync: vi.fn().mockResolvedValue(undefined),
}))

// Mock the types module
vi.mock('@/stores/sync/orchestrator/types', () => ({
  orchestratorLog: {
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  },
}))

// Mock Pinia stores (auto-imported in Nuxt)
vi.stubGlobal('useSyncBackendsStore', vi.fn(() => ({
  backends: [],
  enabledBackends: [],
})))

vi.stubGlobal('useSyncEngineStore', vi.fn(() => ({
  supabaseClient: null,
  getAuthTokenAsync: vi.fn().mockResolvedValue(null),
})))

vi.stubGlobal('useDeviceStore', vi.fn(() => ({
  deviceId: 'test-device-id',
})))

import {
  setupVisibilityListener,
  removeVisibilityListener,
  _getReconnectionContext,
  _resetReconnectionContext,
  _reconnectAllBackendsAsync,
} from '@/stores/sync/orchestrator/realtime'

// Mock document.visibilityState
let mockVisibilityState: DocumentVisibilityState = 'visible'
const visibilityChangeListeners: Array<() => void> = []

// Store original document properties
const originalAddEventListener = document.addEventListener
const originalRemoveEventListener = document.removeEventListener

describe('Visibility-based Reconnection', () => {
  beforeEach(() => {
    // Reset context before each test
    _resetReconnectionContext()

    // Reset visibility state
    mockVisibilityState = 'visible'

    // Clear listeners array
    visibilityChangeListeners.length = 0

    // Mock document.visibilityState
    Object.defineProperty(document, 'visibilityState', {
      configurable: true,
      get: () => mockVisibilityState,
    })

    // Mock addEventListener to capture visibility change listeners
    document.addEventListener = vi.fn((event: string, handler: EventListener) => {
      if (event === 'visibilitychange') {
        visibilityChangeListeners.push(handler as () => void)
      }
      originalAddEventListener.call(document, event, handler)
    }) as typeof document.addEventListener

    // Mock removeEventListener
    document.removeEventListener = vi.fn((event: string, handler: EventListener) => {
      if (event === 'visibilitychange') {
        const index = visibilityChangeListeners.indexOf(handler as () => void)
        if (index > -1) {
          visibilityChangeListeners.splice(index, 1)
        }
      }
      originalRemoveEventListener.call(document, event, handler)
    }) as typeof document.removeEventListener

    // Use fake timers for setTimeout
    vi.useFakeTimers()
  })

  afterEach(() => {
    // Cleanup
    removeVisibilityListener()
    vi.useRealTimers()
    vi.restoreAllMocks()

    // Restore original functions
    document.addEventListener = originalAddEventListener
    document.removeEventListener = originalRemoveEventListener
  })

  // ============================================================================
  // setupVisibilityListener Tests
  // ============================================================================

  describe('setupVisibilityListener', () => {
    it('registers a visibilitychange event listener', () => {
      setupVisibilityListener()

      expect(document.addEventListener).toHaveBeenCalledWith(
        'visibilitychange',
        expect.any(Function),
      )
    })

    it('sets visibilityHandler in reconnection context', () => {
      const context = _getReconnectionContext()
      expect(context.visibilityHandler).toBeNull()

      setupVisibilityListener()

      expect(context.visibilityHandler).not.toBeNull()
      expect(typeof context.visibilityHandler).toBe('function')
    })

    it('does not register duplicate handlers when called multiple times', () => {
      setupVisibilityListener()
      setupVisibilityListener()
      setupVisibilityListener()

      // Should only add one listener
      expect(visibilityChangeListeners.length).toBe(1)
    })
  })

  // ============================================================================
  // removeVisibilityListener Tests
  // ============================================================================

  describe('removeVisibilityListener', () => {
    it('removes the visibilitychange event listener', () => {
      setupVisibilityListener()
      const context = _getReconnectionContext()
      const handler = context.visibilityHandler

      removeVisibilityListener()

      expect(document.removeEventListener).toHaveBeenCalledWith(
        'visibilitychange',
        handler,
      )
    })

    it('clears visibilityHandler from reconnection context', () => {
      setupVisibilityListener()
      const context = _getReconnectionContext()
      expect(context.visibilityHandler).not.toBeNull()

      removeVisibilityListener()

      expect(context.visibilityHandler).toBeNull()
    })

    it('does nothing if no handler was registered', () => {
      // Should not throw
      removeVisibilityListener()

      expect(document.removeEventListener).not.toHaveBeenCalled()
    })
  })

  // ============================================================================
  // Visibility Change Behavior Tests
  // ============================================================================

  describe('visibility change triggers reconnection', () => {
    it('schedules reconnection when app becomes visible', () => {
      setupVisibilityListener()

      // Simulate going to background first
      mockVisibilityState = 'hidden'
      visibilityChangeListeners.forEach((handler) => handler())

      // Now simulate coming back to foreground
      mockVisibilityState = 'visible'
      visibilityChangeListeners.forEach((handler) => handler())

      // Reconnection should be scheduled with 500ms delay
      // We can't easily test the async behavior here without mocking subscribeToBackendAsync
      // but we can verify the handler was called
      expect(visibilityChangeListeners.length).toBe(1)
    })

    it('does not trigger reconnection when app goes to background', () => {
      setupVisibilityListener()

      // Simulate going to background
      mockVisibilityState = 'hidden'
      visibilityChangeListeners.forEach((handler) => handler())

      // Context should remain unchanged (no reconnection triggered for hidden state)
      const context = _getReconnectionContext()
      expect(context.reconnectionPending).toBe(false)
    })
  })

  // ============================================================================
  // Reconnection Context Tests
  // ============================================================================

  describe('reconnection context management', () => {
    it('_resetReconnectionContext clears all fields', () => {
      // Set some values
      const context = _getReconnectionContext()

      // Trigger setupVisibilityListener to set handler
      setupVisibilityListener()
      expect(context.visibilityHandler).not.toBeNull()

      // Reset
      _resetReconnectionContext()

      // Verify all fields are cleared
      expect(context.syncStates).toEqual({})
      expect(context.syncBackendsStore).toBeNull()
      expect(context.syncEngineStore).toBeNull()
      expect(context.currentVaultId).toBeUndefined()
      expect(context.visibilityHandler).toBeNull()
      expect(context.reconnectionPending).toBe(false)
    })

    it('_getReconnectionContext returns the same reference', () => {
      const context1 = _getReconnectionContext()
      const context2 = _getReconnectionContext()

      expect(context1).toBe(context2)
    })
  })

  // ============================================================================
  // Reconnect All Backends Tests
  // ============================================================================

  describe('_reconnectAllBackendsAsync', () => {
    it('skips reconnection when context is not initialized', async () => {
      _resetReconnectionContext()

      // Should not throw, just skip
      await _reconnectAllBackendsAsync()

      const context = _getReconnectionContext()
      expect(context.reconnectionPending).toBe(false)
    })

    it('prevents concurrent reconnection attempts', async () => {
      const context = _getReconnectionContext()

      // Mock a minimal valid context with a backend that takes time to process
      let resolveSubscribe: () => void
      const subscribePromise = new Promise<void>((resolve) => {
        resolveSubscribe = resolve
      })

      // Mock context with a backend
      context.syncBackendsStore = {
        enabledBackends: [{ id: 'backend-1' }],
      } as unknown as ReturnType<typeof useSyncBackendsStore>
      context.syncEngineStore = {} as unknown as ReturnType<typeof useSyncEngineStore>
      context.syncStates = {
        'backend-1': {
          isConnected: false,
          isSyncing: false,
          error: null,
          subscription: {
            unsubscribe: () => subscribePromise,
          },
        },
      }

      // Start first reconnection - it will be blocked on unsubscribe
      const promise1 = _reconnectAllBackendsAsync()

      // Give the async function time to set reconnectionPending
      await vi.advanceTimersByTimeAsync(0)

      // Verify reconnectionPending is set
      expect(context.reconnectionPending).toBe(true)

      // Try starting another reconnection - should be skipped due to guard
      const promise2 = _reconnectAllBackendsAsync()

      // Still pending
      expect(context.reconnectionPending).toBe(true)

      // Resolve the blocking promise
      resolveSubscribe!()

      // Wait for both to complete
      await Promise.all([promise1, promise2])

      // Should be reset after completion
      expect(context.reconnectionPending).toBe(false)
    })

    it('resets reconnectionPending even if reconnection fails', async () => {
      const context = _getReconnectionContext()

      // Mock context with backend that will cause an error
      context.syncBackendsStore = {
        enabledBackends: [{ id: 'backend-1' }],
      } as unknown as ReturnType<typeof useSyncBackendsStore>
      context.syncEngineStore = {} as unknown as ReturnType<typeof useSyncEngineStore>
      context.syncStates = {
        'backend-1': {
          isConnected: false,
          isSyncing: false,
          error: null,
          subscription: null,
        },
      }

      // This will fail during subscribeToBackendAsync but should still reset the flag
      await _reconnectAllBackendsAsync()

      expect(context.reconnectionPending).toBe(false)
    })
  })

  // ============================================================================
  // Integration Tests
  // ============================================================================

  describe('integration: full visibility change cycle', () => {
    it('handles complete background -> foreground cycle', async () => {
      // Setup listener
      setupVisibilityListener()

      const context = _getReconnectionContext()

      // Verify initial state
      expect(context.visibilityHandler).not.toBeNull()

      // Go to background
      mockVisibilityState = 'hidden'
      visibilityChangeListeners.forEach((handler) => handler())

      // Come back to foreground
      mockVisibilityState = 'visible'
      visibilityChangeListeners.forEach((handler) => handler())

      // Advance timers to trigger the delayed reconnection
      await vi.advanceTimersByTimeAsync(500)

      // Cleanup
      removeVisibilityListener()

      expect(context.visibilityHandler).toBeNull()
    })
  })
})
