import { onMounted, onUnmounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { platform } from '@tauri-apps/plugin-os'
import { getCurrentWindow } from '@tauri-apps/api/window'

/**
 * Handles Android back button to navigate within the app instead of closing it
 * Mimics browser behavior: navigate back if possible, close app if on first page
 */
export function useAndroidBackButton() {
  const router = useRouter()
  const historyStack = ref<string[]>([])
  let unlisten: (() => void) | null = null

  // Track navigation history manually
  router.afterEach((to, from) => {
    // If navigating forward (new page)
    if (
      from.path &&
      to.path !== from.path &&
      !historyStack.value.includes(to.path)
    ) {
      historyStack.value.push(from.path)
    }
  })

  onMounted(async () => {
    const os = platform()

    if (os === 'android') {
      const appWindow = getCurrentWindow()

      // Listen to close requested event (triggered by Android back button)
      unlisten = await appWindow.onCloseRequested(async (event) => {
        // Check if we have history
        if (historyStack.value.length > 0) {
          // Prevent window from closing
          event.preventDefault()

          // Remove current page from stack
          historyStack.value.pop()

          // Navigate back in router
          router.back()
        }
        // If no history, allow default behavior (app closes)
      })
    }
  })

  onUnmounted(() => {
    if (unlisten) {
      unlisten()
    }
  })
}
