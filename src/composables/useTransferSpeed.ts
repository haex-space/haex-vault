import {
  downloadSpeed,
  SPEED_WINDOW_MS,
  type ProgressSample,
} from '~/lib/transferSpeed'

/**
 * The speed of a running transfer in bytes per second, from how many bytes it reports so far:
 * `bytes` returns that count, or `null` while nothing runs (the samples are then dropped). The
 * speed is `null` until there are enough samples, and again when the reports stop coming, because
 * a clock ticks once a second and old samples fall out of the window. Call it in `setup`; the
 * clock stops with the component.
 */
export function useTransferSpeed(bytes: () => number | null | undefined) {
  const samples = ref<ProgressSample[]>([])
  const now = ref(Date.now())
  let ticker: ReturnType<typeof setInterval> | undefined

  onMounted(() => {
    ticker = setInterval(() => {
      now.value = Date.now()
    }, 1000)
  })

  onBeforeUnmount(() => {
    clearInterval(ticker)
  })

  watch(bytes, (done) => {
    if (done === null || done === undefined) {
      samples.value = []
      return
    }
    const at = Date.now()
    now.value = at
    // A count below an earlier one is a new transfer: the old samples say nothing about it.
    const kept = samples.value.filter(
      (sample) => sample.at >= at - SPEED_WINDOW_MS && sample.bytes <= done,
    )
    samples.value = [...kept, { at, bytes: done }]
  })

  return computed(() => downloadSpeed(samples.value, now.value))
}
