import { hostname as tauriHostname } from '@tauri-apps/plugin-os'
import { invoke } from '@tauri-apps/api/core'
import {
  getPlatform,
  isDesktop as isDesktopPlatform,
  isMobile as isMobilePlatform,
} from '~/utils/platform'
import { haexSpaceDevices } from '~/database/schemas'
import { createLogger } from '@/stores/logging'
import type { DeviceResolution } from '~/../src-tauri/bindings/DeviceResolution'
import type { DeviceCreated } from '~/../src-tauri/bindings/DeviceCreated'
import type { KnownDevice as KnownDeviceBinding } from '~/../src-tauri/bindings/KnownDevice'

export interface KnownDevice {
  /** `haex_devices.id` — random PK, opaque FK target. */
  rowId: string
  /** Stable file UUID from `<app_data>/device_id`. */
  deviceId: string
  /** iroh ed25519 public key for this device in the current vault. */
  endpointId: string
  name: string
  platform: string
  avatar?: string
  avatarOptions?: string
  isCurrentDevice: boolean
}

const log = createLogger('DEVICE')

export const useDeviceStore = defineStore('vaultDeviceStore', () => {
  /**
   * `haex_devices.id` for the current physical device on this vault.
   * Used as FK in `haex_space_devices.deviceId` and `haex_peer_shares.deviceId`.
   */
  const deviceRowId = ref<string>('')

  /**
   * iroh ed25519 public key for this (device × vault). Per-vault distinct
   * so peers cannot correlate the same device across vaults via EndpointId.
   * The frontend keeps the legacy name `deviceId` for this value because it
   * is used everywhere as the "who am I on the wire" identifier.
   */
  const deviceId = ref<string>('')

  /**
   * Stable file UUID from `<app_data>/device_id`. Same across all vaults of
   * this user on the same physical device. Lives only inside the vault as
   * `haex_devices.device_id` — never published in `haex_space_devices`.
   */
  const localDeviceId = ref<string>('')

  const platform = computed(() => getPlatform())

  /** True for mobile platforms (iOS, Android) */
  const isMobile = computed(() => isMobilePlatform())

  /** True for desktop platforms (everything except iOS/Android) */
  const isDesktop = computed(() => isDesktopPlatform())

  const hostname = computedAsync(() => tauriHostname())

  const deviceName = ref<string>()

  /** Map of all known devices in this vault (endpointId → KnownDevice). */
  const knownDevices = ref<Map<string, KnownDevice>>(new Map())

  /**
   * Pending reconciliation state, set by `resolveAsync` when the open vault
   * does not yet have a `haex_devices` row for this physical device. The UI
   * is expected to read this and surface the Reconciliation dialog.
   */
  const pendingResolution = ref<DeviceResolution | null>(null)

  /**
   * Resolve the open vault against `<app_data>/device_id`.
   *
   * - Matched: silently loads the endpoint key and reports `'matched'`.
   * - New device: stores the resolution in `pendingResolution` and reports
   *   `'pending'`. The caller must then drive the Reconciliation dialog and
   *   call `registerNewAsync` or `reclaimAsync` to commit a choice.
   */
  const resolveAsync = async (): Promise<'matched' | 'pending'> => {
    const res = await invoke<DeviceResolution>('device_resolve_for_vault')
    localDeviceId.value = res.deviceId
    if (res.matchedId && res.matchedEndpointId) {
      deviceRowId.value = res.matchedId
      deviceId.value = res.matchedEndpointId
      pendingResolution.value = null
      await loadEndpointKeyAsync()
      await updateDeviceClaimsAsync()
      return 'matched'
    }
    pendingResolution.value = res
    return 'pending'
  }

  /**
   * Resolve the DID of the own identity that should own newly registered
   * device rows. Throws when there is no own identity yet — `initVaultAsync`
   * always seeds one before driving device resolution, so hitting this is a
   * setup bug, not a normal flow.
   */
  const requireOwnDidAsync = async (): Promise<string> => {
    const identityStore = useIdentityStore()
    if (identityStore.ownIdentities.length === 0) {
      await identityStore.loadIdentitiesAsync()
    }
    const own = identityStore.ownIdentities[0]
    if (!own) {
      throw new Error('device store: no own identity available to own the device row')
    }
    return own.did
  }

  /** Register a brand-new `haex_devices` row for this physical device. */
  const registerNewAsync = async (
    name: string,
    avatar?: string,
    avatarOptions?: string,
  ): Promise<DeviceCreated> => {
    const ownerDid = await requireOwnDidAsync()
    const res = await invoke<DeviceCreated>('device_create_for_vault', {
      ownerDid,
      name,
      platform: platform.value,
      avatar,
      avatarOptions,
    })
    deviceRowId.value = res.id
    deviceId.value = res.endpointId
    localDeviceId.value = res.deviceId
    pendingResolution.value = null
    await loadEndpointKeyAsync()
    await updateDeviceClaimsAsync()
    return res
  }

  /** Reclaim an existing `haex_devices` row for this physical device. */
  const reclaimAsync = async (
    existingId: string,
    name?: string,
    avatar?: string,
    avatarOptions?: string,
  ): Promise<DeviceCreated> => {
    const ownerDid = await requireOwnDidAsync()
    const res = await invoke<DeviceCreated>('device_reclaim_existing', {
      existingId,
      ownerDid,
      name,
      platform: platform.value,
      avatar,
      avatarOptions,
    })
    deviceRowId.value = res.id
    deviceId.value = res.endpointId
    localDeviceId.value = res.deviceId
    pendingResolution.value = null
    await loadEndpointKeyAsync()
    await updateDeviceClaimsAsync()
    return res
  }

  /**
   * Dismiss the pending reconciliation without committing a choice. The
   * vault keeps running without a haex_devices row and P2P stays down for
   * this session; the Geräte-&-Spaces settings page can revisit the choice.
   */
  const skipResolution = () => {
    pendingResolution.value = null
  }

  /** Push the current device row's secret key into the iroh endpoint. */
  const loadEndpointKeyAsync = async () => {
    if (!deviceRowId.value) return
    const eid = await invoke<string>('endpoint_load_for_device', {
      deviceRowId: deviceRowId.value,
    })
    deviceId.value = eid
  }

  /**
   * Add or update device:<hostname> claims on all identities so the
   * device endpoint ID is always associated with the identity.
   * Called from resolveAsync/registerNewAsync and when identities are synced
   * from another device.
   */
  const updateDeviceClaimsAsync = async () => {
    if (!deviceId.value) return

    const identityStore = useIdentityStore()
    const name = deviceName.value || hostname.value || 'device'
    const claimType = `device:${name}`

    for (const identity of identityStore.ownIdentities) {
      try {
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

  /** Load all known devices from haex_space_devices + current device. */
  const loadKnownDevicesAsync = async () => {
    const map = new Map<string, KnownDevice>()

    if (deviceId.value) {
      map.set(deviceId.value, {
        rowId: deviceRowId.value,
        deviceId: localDeviceId.value,
        endpointId: deviceId.value,
        name: hostname.value || deviceName.value || deviceId.value.slice(0, 12),
        platform: platform.value,
        isCurrentDevice: true,
      })
    }

    const vaultStore = useVaultStore()
    if (vaultStore.currentVault?.drizzle) {
      try {
        const devices = await vaultStore.currentVault.drizzle
          .select()
          .from(haexSpaceDevices)
        for (const d of devices) {
          if (!map.has(d.endpointId)) {
            map.set(d.endpointId, {
              rowId: d.deviceId,
              deviceId: '', // unknown: not replicated to peers
              endpointId: d.endpointId,
              name: d.name,
              platform: d.platform,
              avatar: d.avatar ?? undefined,
              avatarOptions: d.avatarOptions ?? undefined,
              isCurrentDevice: false,
            })
          }
        }
      } catch { /* table might not exist yet */ }
    }

    knownDevices.value = map
  }

  /** Get display name for a device (matched on endpoint id). */
  const getDeviceName = (id: string): string => {
    return knownDevices.value.get(id)?.name || id.slice(0, 12) + '...'
  }

  const reset = () => {
    deviceRowId.value = ''
    deviceId.value = ''
    localDeviceId.value = ''
    deviceName.value = undefined
    knownDevices.value = new Map()
    pendingResolution.value = null
  }

  return {
    deviceRowId,
    deviceId,
    localDeviceId,
    deviceName,
    pendingResolution,
    getDeviceName,
    hostname,
    resolveAsync,
    registerNewAsync,
    reclaimAsync,
    skipResolution,
    loadEndpointKeyAsync,
    updateDeviceClaimsAsync,
    isDesktop,
    isMobile,
    knownDevices,
    loadKnownDevicesAsync,
    platform,
    reset,
  }
})

export type { KnownDeviceBinding }
