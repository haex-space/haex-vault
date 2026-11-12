import { invoke } from '@tauri-apps/api/core'

/**
 * Full brightness and a screen that stays on while a code is shown large (spec 092, FR-008): on
 * Android through the window (`screen_set_bright`, `screen_set_keep_on`), elsewhere through the
 * Screen Wake Lock API of the webview where it has one. Both are given back when the view closes,
 * when its component goes away (a closed tab) and while the window is hidden or in the background;
 * when the window is visible again and the view is still wanted, both come back. Where a platform
 * has neither, nothing happens and nothing throws: the code is shown large either way.
 */
export function useScreenBright() {
  const wanted = ref(false)
  let wakeLock: WakeLockSentinel | null = null
  // The changes run one after the other, in the order they were asked for: a request to turn the
  // screen settings on that is still waiting when the view closes is followed by the request to
  // turn them off, never overtaken by it, so nothing stays on after the view is gone.
  let queue: Promise<void> = Promise.resolve()

  async function applyAsync(on: boolean) {
    await invoke('screen_set_bright', { on }).catch(() => undefined)
    await invoke('screen_set_keep_on', { on }).catch(() => undefined)
    try {
      if (on) {
        wakeLock ??= (await navigator.wakeLock?.request('screen')) ?? null
      } else if (wakeLock) {
        const held = wakeLock
        wakeLock = null
        await held.release()
      }
    } catch {
      // Not allowed or not available: the code is still shown large.
      wakeLock = null
    }
  }

  function enqueue(on: boolean): Promise<void> {
    queue = queue.then(() => applyAsync(on))
    return queue
  }

  async function onAsync() {
    wanted.value = true
    await enqueue(true)
  }

  async function offAsync() {
    wanted.value = false
    await enqueue(false)
  }

  function onVisibility() {
    if (!wanted.value) return
    void enqueue(document.visibilityState === 'visible')
  }

  onMounted(() => document.addEventListener('visibilitychange', onVisibility))
  onBeforeUnmount(() => {
    document.removeEventListener('visibilitychange', onVisibility)
    if (wanted.value) void offAsync()
  })

  return { wanted, onAsync, offAsync }
}
