/**
 * Centralized back navigation handler.
 * Manages a stack of undo actions triggered by browser back button,
 * mouse back button, or Android back gesture.
 *
 * When the stack is empty, back navigation is blocked — the user
 * stays in the vault and can only leave via the explicit close button.
 */

interface BackAction {
  undo: () => void
}

const backStack: BackAction[] = []
let listenerRegistered = false

function handlePopstate() {
  const action = backStack.pop()
  if (action) {
    action.undo()
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

  const pushBack = (action: BackAction) => {
    backStack.push(action)
    window.history.pushState({ backNavIndex: backStack.length }, '')
  }

  return { pushBack }
}
