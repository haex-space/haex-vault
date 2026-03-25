/**
 * Composable for drill-down navigation with back/forward support.
 * Manages a view stack with useBackNavigation integration.
 *
 * @param defaultView - The initial/root view identifier
 */
export function useDrillDownNavigation<T extends string>(defaultView: T) {
  const { pushBack } = useBackNavigation()
  const activeView = ref<T>(defaultView) as Ref<T>

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
