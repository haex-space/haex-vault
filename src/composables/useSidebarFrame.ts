import type { Ref } from 'vue'
import { WM_OVERFLOW_OPEN } from '~/lib/wm/overlay'

/**
 * The sidebar of an app frame that is a size container (chat, settings, passwords): from `@2xl`
 * the sidebar sits beside the content and can be hidden, below that it is gone and opens over the
 * content. Whether the frame is wide is read from the panel's CSS (`position`) rather than from a
 * second copy of the threshold. Nothing is kept — the state is local to the open tab. A sidebar that
 * covers the content and the window's overflow panel (spec 090) are never open together: opening
 * one closes the other.
 *
 * `panel` returns the sidebar's root element, which the frame styles `absolute` below `@2xl`.
 */
export function useSidebarFrame(
  frame: Ref<HTMLElement | null>,
  panel: () => HTMLElement | null | undefined,
) {
  /** Wide window: the operator hid the sidebar. */
  const wideHidden = ref(false)
  /** Narrow window: the sidebar is open over the content. */
  const menuOpen = ref(false)
  const wide = ref(true)

  function isWide(): boolean {
    const el = panel()
    return el ? getComputedStyle(el).position !== 'absolute' : true
  }

  const { width } = useElementSize(frame)
  watch(width, () => {
    wide.value = isWide()
    if (wide.value) menuOpen.value = false
  })
  onMounted(() => {
    wide.value = isWide()
  })

  const overflowOpen = inject(WM_OVERFLOW_OPEN, null)
  watch(
    () => overflowOpen?.value,
    (open) => {
      if (open) menuOpen.value = false
    },
  )

  const visible = computed(() =>
    wide.value ? !wideHidden.value : menuOpen.value,
  )
  /** The sidebar covers the content, which must not take focus meanwhile. */
  const overlaying = computed(() => menuOpen.value && !wide.value)

  function show() {
    if (isWide()) wideHidden.value = false
    else {
      menuOpen.value = true
      if (overflowOpen) overflowOpen.value = false
    }
  }
  function hide() {
    if (isWide()) wideHidden.value = true
    else menuOpen.value = false
  }
  function toggle() {
    if (visible.value) hide()
    else show()
  }
  /** The overlay closes, back to the content; a wide window keeps its sidebar. */
  function close() {
    menuOpen.value = false
  }

  return {
    wideHidden,
    menuOpen,
    wide,
    visible,
    overlaying,
    isWide,
    show,
    hide,
    toggle,
    close,
  }
}
