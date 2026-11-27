/** How often the relative times ("48 Min.", "vor 2 Tagen") are looked at again. */
const REFRESH_MS = 30_000

// One clock for every relative time on screen: it runs while at least one of them is mounted.
const now = shallowRef(new Date())
let watchers = 0
let timer: ReturnType<typeof setInterval> | null = null

/**
 * The current time, ticking every 30 s, shared by all callers so a list of times costs one timer.
 * A relative time that reads it follows the clock without being reloaded.
 */
export function useSharedNow() {
  if (watchers++ === 0) {
    now.value = new Date()
    timer = setInterval(() => {
      now.value = new Date()
    }, REFRESH_MS)
  }
  onScopeDispose(() => {
    if (--watchers === 0 && timer !== null) {
      clearInterval(timer)
      timer = null
    }
  })
  return readonly(now)
}
