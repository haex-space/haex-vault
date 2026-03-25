/**
 * Composable for drill-down navigation with back/forward support.
 * Uses a shared registry so undo/redo closures always reference the
 * current active ref, even after component remounts.
 *
 * @param id - Unique identifier for this navigation scope
 * @param defaultView - The initial/root view identifier
 */

const registry = new Map<string, Ref<string>>()

export function useDrillDownNavigation<T extends string>(defaultView: T, id?: string) {
  const { pushBack } = useBackNavigation()

  // Use a stable key: explicit id or generate from default view
  const key = id ?? defaultView

  // Reuse existing ref if component was remounted, preserving undo/redo state
  if (!registry.has(key)) {
    registry.set(key, ref<string>(defaultView) as Ref<string>)
  }
  const activeView = registry.get(key) as Ref<T>

  const navigateTo = (view: T) => {
    if (view === activeView.value) return

    const previous = activeView.value
    activeView.value = view

    pushBack({
      undo: () => { activeView.value = previous },
      redo: () => { activeView.value = view },
    })
  }

  const goBack = () => {
    window.history.back()
  }

  return { activeView, navigateTo, goBack }
}
