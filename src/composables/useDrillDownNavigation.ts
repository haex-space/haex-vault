/**
 * Composable for drill-down navigation with per-tab back/forward support.
 * Uses a shared registry so undo/redo closures always reference the
 * current active ref, even after component remounts.
 *
 * On component unmount the registry entry is cleared so subsequent mounts
 * start from the default view. Without this, navigating away from a
 * settings category and back would show the last active subview instead
 * of the overview (visible on production builds where HMR does not reset
 * the module-level registry).
 *
 * @param defaultView - The initial/root view identifier
 * @param id          - Unique identifier for this navigation scope
 * @param tabId       - Tab ID for per-tab navigation scoping
 */

const registry = new Map<string, Ref<string>>()

export function useDrillDownNavigation<T extends string>(
  defaultView: T,
  id: string,
  tabId: string,
) {
  const navigationStore = useNavigationStore()

  // Include tabId in registry key so each tab has independent drill-down state
  const key = `${id}-${tabId}`

  // Reuse existing ref if component was remounted, preserving undo/redo state
  if (!registry.has(key)) {
    registry.set(key, ref<string>(defaultView) as Ref<string>)
  }
  const activeView = registry.get(key) as Ref<T>

  const navigateTo = (view: T) => {
    if (view === activeView.value) return

    const previous = activeView.value
    activeView.value = view

    navigationStore.pushBack({
      undo: () => { activeView.value = previous },
      redo: () => { activeView.value = view },
    }, tabId)
  }

  const goBack = () => {
    window.history.back()
  }

  // Clean up on unmount so the next mount starts from the default view
  onUnmounted(() => {
    registry.delete(key)
  })

  return { activeView, navigateTo, goBack }
}
