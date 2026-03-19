/**
 * Reactive window-level size breakpoints.
 *
 * Works like `isSmallScreen` from the UI store but scoped to a single
 * window's width — useful for adapting layouts inside the window manager.
 *
 * Breakpoint thresholds mirror Tailwind's @container query sizes so
 * JS logic and CSS container queries stay in sync.
 */
export const useWindowSize = (windowId: string | Ref<string>) => {
  const windowManager = useWindowManagerStore()

  const win = computed(() =>
    windowManager.windows.find((w) => w.id === unref(windowId)),
  )

  /** Window width in pixels (0 when not found) */
  const windowWidth = computed(() => win.value?.width ?? 0)

  /** Window height in pixels (0 when not found) */
  const windowHeight = computed(() => win.value?.height ?? 0)

  // Tailwind @container breakpoints: @sm = 640, @md = 768, @lg = 1024
  /** true when window is narrower than 640px (@container sm) */
  const isCompact = computed(() => windowWidth.value < 640)

  /** true when window is narrower than 768px (@container md) */
  const isNarrow = computed(() => windowWidth.value < 768)

  /** true when window is at least 1024px wide (@container lg) */
  const isWide = computed(() => windowWidth.value >= 1024)

  return {
    windowWidth,
    windowHeight,
    isCompact,
    isNarrow,
    isWide,
  }
}
