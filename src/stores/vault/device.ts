import { hostname as tauriHostname } from '@tauri-apps/plugin-os'
import { invoke } from '@tauri-apps/api/core'
import {
  getPlatform,
  isDesktop as isDesktopPlatform,
  isMobile as isMobilePlatform,
} from '~/utils/platform'
import { haexSpaceDevices } from '~/database/schemas'
import { createLogger } from '@/stores/logging'

export interface KnownDevice {
  deviceId: string
  name: string
  isCurrentDevice: boolean
}

const log = createLogger('DEVICE')

export const useDeviceStore = defineStore('vaultDeviceStore', () => {
  const deviceId = ref<string>('')

  const platform = computed(() => getPlatform())

  /** True for mobile platforms (iOS, Android) */
  const isMobile = computed(() => isMobilePlatform())

  /** True for desktop platforms (everything except iOS/Android) */
  const isDesktop = computed(() => isDesktopPlatform())

  const hostname = computedAsync(() => tauriHostname())

  const deviceName = ref<string>()

  /** Map of all known devices (deviceEndpointId → KnownDevice) */
  const knownDevices = ref<Map<string, KnownDevice>>(new Map())

  /**
   * Initializes the device identity by calling the Rust device_init_key command.
   * Returns the EndpointId (Ed25519 public key) which uniquely identifies
   * this device for the current vault.
   */
  const initDeviceIdAsync = async () => {
    const endpointId = await invoke<string>('device_init_key')
    deviceId.value = endpointId
    await updateDeviceClaimsAsync()
    return endpointId
  }

  /**
   * Add or update device:<hostname> claims on all identities so the
   * device endpoint ID is always associated with the identity.
   * Called on device init and when identities are synced from another device.
   */
  const updateDeviceClaimsAsync = async () => {
    if (!deviceId.value) return

    const identityStore = useIdentityStore()
    const name = deviceName.value || hostname.value || 'device'
    const claimType = `device:${name}`

    for (const identity of identityStore.ownIdentities) {
      try {
        // Verify identity exists in DB before touching claims
        const dbIdentity = await identityStore.getIdentityByIdAsync(identity.id)
        if (!dbIdentity) continue

        const claims = await identityStore.getClaimsAsync(identity.id)
        const existing = claims.find(c => c.type === claimType)

        if (existing && existing.value === deviceId.value) continue

        if (existing) {
          await identityStore.updateClaimAsync(existing.id, deviceId.value)
        } else {
          await identityStore.addClaimAsync(identity.id, claimType, deviceId.value)
        }
      } catch (e) {
        log.warn(`Failed to update device claim for identity:`, e)
      }
    }
  }

  /** Load all known devices from space_devices table + current device */
  const loadKnownDevicesAsync = async () => {
    const map = new Map<string, KnownDevice>()

    // Current device
    if (deviceId.value) {
      map.set(deviceId.value, {
        deviceId: deviceId.value,
        name: hostname.value || deviceName.value || deviceId.value.slice(0, 12),
        isCurrentDevice: true,
      })
    }

    // Other devices from space_devices table
    const vaultStore = useVaultStore()
    if (vaultStore.currentVault?.drizzle) {
      try {
        const devices = await vaultStore.currentVault.drizzle
          .select()
          .from(haexSpaceDevices)
        for (const d of devices) {
          if (!map.has(d.deviceEndpointId)) {
            map.set(d.deviceEndpointId, {
              deviceId: d.deviceEndpointId,
              name: d.deviceName,
              isCurrentDevice: false,
            })
          }
        }
      } catch { /* table might not exist yet */ }
    }

    knownDevices.value = map
  }

  /** Get display name for a device ID */
  const getDeviceName = (id: string): string => {
    return knownDevices.value.get(id)?.name || id.slice(0, 12) + '...'
  }

  const reset = () => {
    deviceId.value = ''
    deviceName.value = undefined
    knownDevices.value = new Map()
  }

  return {
    deviceId,
    deviceName,
    getDeviceName,
    hostname,
    initDeviceIdAsync,
    updateDeviceClaimsAsync,
    isDesktop,
    isMobile,
    knownDevices,
    loadKnownDevicesAsync,
    platform,
    reset,
  }
})
