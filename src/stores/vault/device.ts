import {
  hostname as tauriHostname,
  platform as tauriPlatform,
} from '@tauri-apps/plugin-os'
import { invoke } from '@tauri-apps/api/core'

export const useDeviceStore = defineStore('vaultDeviceStore', () => {
  const deviceId = ref<string>('')

  const platform = computedAsync(() => tauriPlatform())

  /** True for mobile platforms (iOS, Android) */
  const isMobile = computed(() => {
    const p = platform.value
    return p === 'ios' || p === 'android'
  })

  /** True for desktop platforms (everything except iOS/Android) */
  const isDesktop = computed(() => !isMobile.value)

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
