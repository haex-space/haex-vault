/**
 * Centralized back/forward navigation handler.
 * Manages undo/redo stacks triggered by browser back/forward button,
 * mouse back/forward button, or Android back gesture.
 *
 * When the back stack is empty, back navigation is blocked — the user
 * stays in the vault and can only leave via the explicit close button.
 */

interface NavAction {
  undo: () => void
  redo: () => void
}

const backStack: NavAction[] = []
const forwardStack: NavAction[] = []
let listenerRegistered = false
let navigatingForward = false

function handlePopstate(event: PopStateEvent) {
  // Detect forward navigation (browser forward button)
  if (event.state?.navDirection === 'forward') {
    const action = forwardStack.pop()
    if (action) {
      navigatingForward = true
      action.redo()
      backStack.push(action)
      navigatingForward = false
    }
    return
  }

  const action = backStack.pop()
  if (action) {
    action.undo()
    forwardStack.push(action)
    // Push forward marker so browser forward button triggers redo
    window.history.pushState({ navDirection: 'forward' }, '')
  } else {
    // Stack empty — prevent navigating away from the vault
    window.history.pushState({ backNavBoundary: true }, '')
  }
}

export function useBackNavigation() {
  if (!listenerRegistered && import.meta.client) {
    window.addEventListener('popstate', handlePopstate)
    listenerRegistered = true

    // Push initial boundary so back can never leave the vault
    window.history.pushState({ backNavBoundary: true }, '')
  }

  /**
   * Register a back/forward action.
   * `undo` is called on back navigation, `redo` on forward.
   * If only `undo` is provided, `redo` defaults to a no-op.
   */
  const pushBack = (action: { undo: () => void; redo?: () => void }) => {
    backStack.push({
      undo: action.undo,
      redo: action.redo ?? (() => {}),
    })
    // New navigation clears forward stack (like a browser)
    if (!navigatingForward) {
      forwardStack.length = 0
    }
    window.history.pushState({ backNavIndex: backStack.length }, '')
  }

  return { pushBack }
}
