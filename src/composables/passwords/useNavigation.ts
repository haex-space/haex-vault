/**
 * Wraps the forward-moving passwords nav actions with `pushBack` entries
 * from the central navigation store, so the browser back button (mouse,
 * Alt+Left, on-screen UI) can restore the prior state. Same pattern as
 * `useDrillDownNavigation` in the settings view, adapted for our
 * multi-field state (selectedItemId + viewMode + isEditing).
 */
export function usePasswordsNavigation(tabIdOverride?: string) {
  const store = usePasswordsStore()
  const groupsStore = usePasswordsGroupsStore()
  const navigationStore = useNavigationStore()
  // Allow passing tabId directly (for the root that provides it); otherwise inject.
  const tabId = tabIdOverride ?? inject<string>('haex-tab-id')

  const withHistory = (apply: () => void, redo: () => void) => {
    const before = store.snapshotNavState()
    apply()
    if (!tabId) return
    navigationStore.pushBack(
      {
        undo: () => store.restoreNavState(before),
        redo,
      },
      tabId,
    )
  }

  const openItem = (itemId: string) => {
    withHistory(
      () => store.openItem(itemId),
      () => store.openItem(itemId),
    )
  }

  const startCreate = () => {
    withHistory(
      () => store.startCreate(),
      () => store.startCreate(),
    )
  }

  const startEdit = () => {
    withHistory(
      () => store.startEdit(),
      () => store.startEdit(),
    )
  }

  /**
   * Switch the active folder and record the change on the tab's back stack —
   * same pattern as `useDrillDownNavigation.navigateTo` in the settings view.
   * If the target equals the currently-selected group, this is a no-op and
   * nothing is pushed onto the stack.
   */
  const selectGroup = (groupId: string | null) => {
    const before = groupsStore.selectedGroupId
    if (before === groupId) return
    groupsStore.selectGroup(groupId)
    if (!tabId) return
    navigationStore.pushBack(
      {
        undo: () => groupsStore.selectGroup(before),
        redo: () => groupsStore.selectGroup(groupId),
      },
      tabId,
    )
  }

  const goBack = () => {
    if (typeof window !== 'undefined') {
      window.history.back()
    } else {
      store.backToList()
    }
  }

  /**
   * Self-rearming sentinel entry on the tab's back stack. It absorbs one
   * back press (undo is a no-op, re-arm for the next) so the global
   * "close window" action in the shared nav store can never be reached
   * via back navigation from within the passwords window.
   */
  const armWindowCloseBoundary = () => {
    if (!tabId) return
    navigationStore.pushBack(
      {
        undo: () => {
          nextTick(() => armWindowCloseBoundary())
        },
        redo: () => {},
      },
      tabId,
    )
  }

  /**
   * Track a reactive value (e.g. active tab) into the back stack.
   * Returns a cleanup function; safe to call again for the same ref.
   */
  const trackHistory = <T>(source: Ref<T>) => {
    let suppress = false
    return watch(source, (next, prev) => {
      if (suppress || !tabId) return
      navigationStore.pushBack(
        {
          undo: () => {
            suppress = true
            source.value = prev
            nextTick(() => {
              suppress = false
            })
          },
          redo: () => {
            suppress = true
            source.value = next
            nextTick(() => {
              suppress = false
            })
          },
        },
        tabId,
      )
    })
  }

  return {
    openItem,
    startCreate,
    startEdit,
    selectGroup,
    goBack,
    armWindowCloseBoundary,
    trackHistory,
  }
}
