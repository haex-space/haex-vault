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

const viewRegistry = new Map<string, Ref<string>>()
const contextRegistry = new Map<string, Ref<Record<string, unknown>>>()
const directionRegistry = new Map<string, Ref<'forward' | 'back'>>()

export function useDrillDownNavigation<T extends string>(
  defaultView: T,
  id: string,
  tabId: string,
) {
  const navigationStore = useNavigationStore()

  // Include tabId in registry key so each tab has independent drill-down state
  const key = `${id}-${tabId}`

  // Reuse existing ref if component was remounted, preserving undo/redo state
  if (!viewRegistry.has(key)) {
    viewRegistry.set(key, ref<string>(defaultView) as Ref<string>)
  }
  if (!contextRegistry.has(key)) {
    contextRegistry.set(key, ref<Record<string, unknown>>({}))
  }
  if (!directionRegistry.has(key)) {
    directionRegistry.set(key, ref<'forward' | 'back'>('forward'))
  }
  const activeView = viewRegistry.get(key) as Ref<T>
  const navigationContext = contextRegistry.get(key) as Ref<Record<string, unknown>>
  const direction = directionRegistry.get(key) as Ref<'forward' | 'back'>

  /**
   * Navigate to a new view, optionally with context data that will be
   * restored on undo/redo (e.g. a selected item ID).
   */
  const navigateTo = (view: T, context?: Record<string, unknown>) => {
    if (view === activeView.value && !context) return

    const previousView = activeView.value
    const previousContext = { ...navigationContext.value }

    direction.value = 'forward'
    activeView.value = view
    if (context) {
      navigationContext.value = context
    }

    navigationStore.pushBack({
      undo: () => {
        direction.value = 'back'
        activeView.value = previousView
        navigationContext.value = previousContext
      },
      redo: () => {
        direction.value = 'forward'
        activeView.value = view
        if (context) {
          navigationContext.value = context
        }
      },
    }, tabId)
  }

  const goBack = () => {
    window.history.back()
  }

  // Clean up on unmount so the next mount starts from the default view
  onUnmounted(() => {
    viewRegistry.delete(key)
    contextRegistry.delete(key)
    directionRegistry.delete(key)
  })

  return { activeView, navigationContext, direction, navigateTo, goBack }
}
