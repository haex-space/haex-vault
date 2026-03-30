/**
 * Centralized per-tab navigation store.
 *
 * Each tab maintains its own back/forward stack so that browser-back,
 * Android back gesture, and mouse back button only affect the
 * currently active tab.  Window open/close actions live on a separate
 * global stack that is used as fallback when the active tab's stack
 * is empty.
 *
 * Forward navigation uses the same `navIndex` trick as the old
 * useBackNavigation composable: each pushBack stores a monotonically
 * increasing index in the browser history state.  On popstate the
 * handler compares the state's index against the current position to
 * distinguish back from forward.
 */

interface NavAction {
  undo: () => void
  redo: () => void
}

export const useNavigationStore = defineStore('navigation', () => {
  // ── Per-tab stacks ──────────────────────────────────────────────
  const tabBackStacks = new Map<string, NavAction[]>()
  const tabForwardStacks = new Map<string, NavAction[]>()

  // ── Global stack (window open/close) ────────────────────────────
  const globalBackStack: NavAction[] = []
  const globalForwardStack: NavAction[] = []

  // Reactive trigger so canGoBack / canGoForward recompute
  const version = ref(0)
  const bump = () => { version.value++ }

  // Monotonically increasing index written into every history state.
  // Compared against the state's stored value to detect direction.
  let navIndex = 0

  // ── Helpers ─────────────────────────────────────────────────────

  function getBackStack(tabId: string): NavAction[] {
    let stack = tabBackStacks.get(tabId)
    if (!stack) {
      stack = []
      tabBackStacks.set(tabId, stack)
    }
    return stack
  }

  function getForwardStack(tabId: string): NavAction[] {
    let stack = tabForwardStacks.get(tabId)
    if (!stack) {
      stack = []
      tabForwardStacks.set(tabId, stack)
    }
    return stack
  }

  /**
   * Resolve the active tab ID by looking up the window manager.
   * Lazy access avoids circular dependency (windowManager → navigation → windowManager).
   */
  function getActiveTabId(): string | null {
    const windowManager = useWindowManagerStore()
    const activeWinId = windowManager.activeWindowId
    if (!activeWinId) return null
    const win = windowManager.windows.find(w => w.id === activeWinId)
    return win?.activeTabId ?? null
  }

  // ── Actions ─────────────────────────────────────────────────────

  /**
   * Register a back/forward action.
   * If `tabId` is provided, the action is scoped to that tab.
   * Otherwise it goes to the global stack (for window open/close).
   */
  function pushBack(action: { undo: () => void; redo?: () => void }, tabId?: string) {
    const full: NavAction = {
      undo: action.undo,
      redo: action.redo ?? (() => {}),
    }

    if (tabId) {
      getBackStack(tabId).push(full)
      getForwardStack(tabId).length = 0
    } else {
      globalBackStack.push(full)
      globalForwardStack.length = 0
    }

    if (import.meta.client) {
      navIndex++
      window.history.pushState({ navIndex }, '')
    }
    bump()
  }

  /** Programmatic back within a specific tab (for UI buttons). */
  function goBack(tabId: string) {
    const stack = tabBackStacks.get(tabId)
    if (!stack?.length) return

    const action = stack.pop()!
    action.undo()
    getForwardStack(tabId).push(action)
    bump()
  }

  /** Programmatic forward within a specific tab (for UI buttons). */
  function goForward(tabId: string) {
    const stack = tabForwardStacks.get(tabId)
    if (!stack?.length) return

    const action = stack.pop()!
    action.redo()
    getBackStack(tabId).push(action)
    bump()
  }

  function canGoBack(tabId: string): boolean {
    // eslint-disable-next-line @typescript-eslint/no-unused-expressions
    version.value // reactive dependency
    return (tabBackStacks.get(tabId)?.length ?? 0) > 0
  }

  function canGoForward(tabId: string): boolean {
    // eslint-disable-next-line @typescript-eslint/no-unused-expressions
    version.value // reactive dependency
    return (tabForwardStacks.get(tabId)?.length ?? 0) > 0
  }

  // ── Cleanup ─────────────────────────────────────────────────────

  function clearTabStacks(tabId: string) {
    tabBackStacks.delete(tabId)
    tabForwardStacks.delete(tabId)
    bump()
  }

  function clearWindowStacks(windowId: string) {
    const windowManager = useWindowManagerStore()
    const win = windowManager.windows.find(w => w.id === windowId)
    if (win) {
      for (const tab of win.tabs) {
        clearTabStacks(tab.id)
      }
    }
  }

  // ── Popstate handler ────────────────────────────────────────────

  function handlePopstate(event: PopStateEvent) {
    const state = event.state
    const stateIndex: number | undefined = state?.navIndex

    // ── Forward navigation ──────────────────────────────────────
    // State index is ahead of our current position → user pressed forward.
    if (stateIndex != null && stateIndex > navIndex) {
      navIndex = stateIndex

      // Try active tab's forward stack
      const tabId = getActiveTabId()
      if (tabId) {
        const fwd = tabForwardStacks.get(tabId)
        if (fwd?.length) {
          const action = fwd.pop()!
          action.redo()
          getBackStack(tabId).push(action)
          bump()
          return
        }
      }

      // Fall back to global forward
      const action = globalForwardStack.pop()
      if (action) {
        action.redo()
        globalBackStack.push(action)
        bump()
      }
      return
    }

    // ── Back navigation ─────────────────────────────────────────
    if (stateIndex != null) {
      navIndex = stateIndex
    }

    // Try active tab's back stack
    const tabId = getActiveTabId()
    if (tabId) {
      const stack = tabBackStacks.get(tabId)
      if (stack?.length) {
        const action = stack.pop()!
        action.undo()
        getForwardStack(tabId).push(action)
        bump()
        return
      }
    }

    // Fall back to global stack (window open/close)
    const action = globalBackStack.pop()
    if (action) {
      action.undo()
      globalForwardStack.push(action)
      bump()
      return
    }

    // Stack empty — prevent navigating away from the vault
    window.history.pushState({ backNavBoundary: true }, '')
  }

  // ── Initialize popstate listener (once, client-side) ────────────

  if (import.meta.client) {
    window.addEventListener('popstate', handlePopstate)
    // Push initial boundary so back can never leave the vault
    window.history.pushState({ backNavBoundary: true }, '')
  }

  const reset = () => {
    tabBackStacks.clear()
    tabForwardStacks.clear()
    globalBackStack.length = 0
    globalForwardStack.length = 0
  }

  return {
    pushBack,
    goBack,
    goForward,
    canGoBack,
    canGoForward,
    clearTabStacks,
    clearWindowStacks,
    reset,
  }
})
