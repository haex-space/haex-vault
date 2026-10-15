import { invoke } from '@tauri-apps/api/core'
import type { PlatformCapabilities } from '~/types/bindings/PlatformCapabilities'

/** Fixed for the life of the process (spec 043, contract platform-capabilities.md): read once. */
const capabilities = shallowRef<PlatformCapabilities | null>(null)
let loading: Promise<PlatformCapabilities | null> | null = null

function loadAsync(): Promise<PlatformCapabilities | null> {
  loading ??= invoke<PlatformCapabilities>('platform_capabilities')
    .then((table) => (capabilities.value = table))
    .catch((error: unknown) => {
      console.warn('platform_capabilities failed', error)
      // A failed read is not the table: the next call asks again, instead of every caller for the
      // life of the page getting the same `null` (the back gesture is registered from the table).
      loading = null
      return null
    })
  return loading
}

/**
 * What this device can do: the backend's table, the one place that decides it. Until the table is
 * there, every facility counts as present (the desktop), so nothing flickers away on a desktop.
 */
export function useDeviceCapabilities() {
  void loadAsync()
  const isAndroid = computed(() => capabilities.value?.platform === 'android')
  return { capabilities, isAndroid, readyAsync: loadAsync }
}
