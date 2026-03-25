/**
 * Composable for drill-down navigation with per-tab back/forward support.
 * Uses a shared registry so undo/redo closures always reference the
 * current active ref, even after component remounts.
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

  return { activeView, navigateTo, goBack }
}
