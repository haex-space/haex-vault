import { onMounted, onUnmounted } from 'vue'
import { platform } from '@tauri-apps/plugin-os'
import { getCurrentWindow } from '@tauri-apps/api/window'

/**
 * Handles Android back button by triggering browser history.back().
 * The navigation store's popstate handler takes care of the rest
 * (per-tab navigation, closing windows, preventing vault exit).
 */
export function useAndroidBackButton() {
  let unlisten: (() => void) | null = null

  onMounted(async () => {
    const os = platform()

    if (os === 'android') {
      const appWindow = getCurrentWindow()

      unlisten = await appWindow.onCloseRequested(async (event) => {
        // Always prevent app close — vault is only closed via explicit button
        event.preventDefault()

        // Trigger browser back which fires popstate → navigation store handles it
        window.history.back()
      })
    }
  })

  onUnmounted(() => {
    if (unlisten) {
      unlisten()
    }
  })
}
