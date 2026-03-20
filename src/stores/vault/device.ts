import { hostname as tauriHostname } from '@tauri-apps/plugin-os'
import { invoke } from '@tauri-apps/api/core'
import {
  getPlatform,
  isDesktop as isDesktopPlatform,
  isMobile as isMobilePlatform,
} from '~/utils/platform'

export const useDeviceStore = defineStore('vaultDeviceStore', () => {
  const deviceId = ref<string>('')

  const platform = computed(() => getPlatform())

  /** True for mobile platforms (iOS, Android) */
  const isMobile = computed(() => isMobilePlatform())

  /** True for desktop platforms (everything except iOS/Android) */
  const isDesktop = computed(() => isDesktopPlatform())

  const hostname = computedAsync(() => tauriHostname())

  const deviceName = ref<string>()

  /**
   * Initializes the device identity by calling the Rust device_init_key command.
   * Returns the EndpointId (Ed25519 public key) which uniquely identifies
   * this device for the current vault.
   */
  const initDeviceIdAsync = async () => {
    const endpointId = await invoke<string>('device_init_key')
    deviceId.value = endpointId
    return endpointId
  }

  return {
    deviceId,
    deviceName,
    hostname,
    initDeviceIdAsync,
    isDesktop,
    isMobile,
    platform,
  }
})
