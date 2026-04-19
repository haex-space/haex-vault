/**
 * Wraps the forward-moving passwords nav actions with `pushBack` entries
 * from the central navigation store, so the browser back button (mouse,
 * Alt+Left, on-screen UI) can restore the prior state. Same pattern as
 * `useDrillDownNavigation` in the settings view, adapted for our
 * multi-field state (selectedItemId + viewMode + isEditing).
 */
export function usePasswordsNavigation() {
  const store = usePasswordsStore()
  const navigationStore = useNavigationStore()
  const tabId = inject<string>('haex-tab-id')

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

  const goBack = () => {
    if (typeof window !== 'undefined') {
      window.history.back()
    } else {
      store.backToList()
    }
  }

  return {
    openItem,
    startCreate,
    startEdit,
    goBack,
  }
}
