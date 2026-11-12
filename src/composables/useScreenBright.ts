import { invoke } from '@tauri-apps/api/core'
import {
  createScreenController,
  newScreenShared,
} from '~/lib/passwords/screenBright'

/** What every view of this window shares: the order of the changes and how many want the screen. */
const shared = newScreenShared()

/**
 * Full brightness and a screen that stays on while a code is shown large (spec 092, FR-008): on
 * Android through the window (`screen_set_bright`, `screen_set_keep_on`), elsewhere through the
 * Screen Wake Lock API of the webview where it has one. Both are given back when the view closes,
 * when its component goes away (a closed tab) and while the window is hidden or in the background;
 * when the window is visible again and the view is still wanted, both come back. Where a platform
 * has neither, nothing happens and nothing throws: the code is shown large either way. The order of
 * the changes, and the rule that the window setting is given back only when no other view wants it,
 * are kept by `createScreenController`.
 */
export function useScreenBright() {
  const wanted = ref(false)
  const controller = createScreenController(
    {
      setNativeAsync: async (on) => {
        await invoke('screen_set_bright', { on }).catch(() => undefined)
        await invoke('screen_set_keep_on', { on }).catch(() => undefined)
      },
      requestLockAsync: async () =>
        (await navigator.wakeLock?.request('screen')) ?? null,
      isVisible: () => document.visibilityState === 'visible',
    },
    shared,
  )

  async function onAsync() {
    wanted.value = true
    await controller.onAsync()
  }

  async function offAsync() {
    wanted.value = false
    await controller.offAsync()
  }

  function onVisibility() {
    void controller.visibilityChangedAsync()
  }

  onMounted(() => document.addEventListener('visibilitychange', onVisibility))
  onBeforeUnmount(() => {
    document.removeEventListener('visibilitychange', onVisibility)
    // The close is queued behind any enable that is still running; that enable sees it.
    if (wanted.value) void offAsync()
  })

  return { wanted, onAsync, offAsync }
}
