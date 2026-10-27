import { fitsSplit } from '~/lib/files/split'

/**
 * Whether two panels fit into the panel area (spec 062 FR-022, research R9): `area` is measured
 * with a `ResizeObserver`. Before the first measurement, and where there is no observer (the Node
 * test harnesses), it fits.
 */
export function useSplitFit(area: Readonly<Ref<HTMLElement | null>>) {
  const width = ref<number | null>(null)
  let observer: ResizeObserver | undefined

  onMounted(() => {
    if (typeof ResizeObserver === 'undefined' || !area.value) return
    observer = new ResizeObserver((entries) => {
      width.value = entries[0]?.contentRect.width ?? null
    })
    observer.observe(area.value)
  })
  onBeforeUnmount(() => observer?.disconnect())

  return computed(() => width.value === null || fitsSplit(width.value))
}
