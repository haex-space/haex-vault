import { invoke } from '@tauri-apps/api/core'
import { eq } from 'drizzle-orm'
import type { PeerStorageStatus } from '~/../src-tauri/bindings/PeerStorageStatus'
import type { FileEntry } from '~/../src-tauri/bindings/FileEntry'
import {
  haexPeerShares,
  haexSpaceDevices,
  type SelectHaexPeerShares,
  type SelectHaexSpaceDevices,
} from '~/database/schemas'

export const usePeerStorageStore = defineStore('peerStorageStore', () => {
  const { currentVault } = storeToRefs(useVaultStore())

  const running = ref(false)
  const nodeId = ref('')
  const shares = ref<SelectHaexPeerShares[]>([])
  const spaceDevices = ref<SelectHaexSpaceDevices[]>([])

  const refreshStatusAsync = async () => {
    try {
      const status = await invoke<PeerStorageStatus>('peer_storage_status')
      running.value = status.running
      nodeId.value = status.nodeId
    } catch (error) {
      console.error('[PeerStorage] Failed to get status:', error)
    }
  }

  // =========================================================================
  // DB-backed share management (via Drizzle / CRDT)
  // =========================================================================

  const loadSharesAsync = async () => {
    const db = currentVault.value?.drizzle
    if (!db) return
    shares.value = await db.select().from(haexPeerShares).all()
  }

  const loadSpaceDevicesAsync = async () => {
    const db = currentVault.value?.drizzle
    if (!db) return
    spaceDevices.value = await db.select().from(haexSpaceDevices).all()
  }

  const addShareAsync = async (spaceId: string, name: string, localPath: string) => {
    const db = currentVault.value?.drizzle
    if (!db) throw new Error('No vault open')
    if (!nodeId.value) throw new Error('Endpoint ID not available — start peer storage first')

    await db.insert(haexPeerShares).values({
      spaceId,
      deviceEndpointId: nodeId.value,
      name,
      localPath,
    })

    await loadSharesAsync()
    await invoke('peer_storage_reload_shares')
  }

  const removeShareAsync = async (shareId: string) => {
    const db = currentVault.value?.drizzle
    if (!db) throw new Error('No vault open')

    await db.delete(haexPeerShares).where(eq(haexPeerShares.id, shareId))

    await loadSharesAsync()
    await invoke('peer_storage_reload_shares')
  }

  // =========================================================================
  // Space device registration
  // =========================================================================

  const registerDeviceInSpaceAsync = async (spaceId: string, deviceName: string, identityPublicKey?: string) => {
    const db = currentVault.value?.drizzle
    if (!db) throw new Error('No vault open')
    if (!nodeId.value) throw new Error('Endpoint ID not available')

    // Resolve identity: use provided or first available
    let identityId = identityPublicKey
    if (!identityId) {
      const identityStore = useIdentityStore()
      identityId = identityStore.identities[0]?.publicKey
    }

    await db.insert(haexSpaceDevices).values({
      spaceId,
      identityId: identityId || null,
      deviceEndpointId: nodeId.value,
      deviceName,
    })

    await loadSpaceDevicesAsync()
  }

  const unregisterDeviceFromSpaceAsync = async (deviceId: string) => {
    const db = currentVault.value?.drizzle
    if (!db) throw new Error('No vault open')

    await db.delete(haexSpaceDevices).where(eq(haexSpaceDevices.id, deviceId))
    await loadSpaceDevicesAsync()
  }

  // =========================================================================
  // Endpoint control
  // =========================================================================

  const startAsync = async () => {
    const id = await invoke<string>('peer_storage_start')
    running.value = true
    nodeId.value = id

    // Load existing devices and auto-register in all spaces
    await loadSpaceDevicesAsync()
    await autoRegisterInSpacesAsync()
    await updateDeviceClaimsAsync()
  }

  const autoRegisterInSpacesAsync = async () => {
    const db = currentVault.value?.drizzle
    if (!db || !nodeId.value) return

    const spacesStore = useSpacesStore()
    const deviceStore = useDeviceStore()
    const hostname = deviceStore.deviceName || deviceStore.hostname || 'Unknown'

    for (const space of spacesStore.spaces) {
      // Check if already registered
      const existing = spaceDevices.value.find(
        d => d.spaceId === space.id && d.deviceEndpointId === nodeId.value,
      )
      if (existing) continue

      try {
        await registerDeviceInSpaceAsync(space.id, hostname)
      } catch (e) {
        console.warn(`[P2P] Failed to register in space ${space.id}:`, e)
      }
    }
  }

  /**
   * Add or update device:hostname claims on all identities so the endpoint ID
   * can be shared via QR code (user chooses whether to include it).
   */
  const updateDeviceClaimsAsync = async () => {
    if (!nodeId.value) return

    const identityStore = useIdentityStore()
    const deviceStore = useDeviceStore()
    const hostname = deviceStore.deviceName || deviceStore.hostname || 'device'
    const claimType = `device:${hostname}`

    for (const identity of identityStore.identities) {
      try {
        const claims = await identityStore.getClaimsAsync(identity.publicKey)
        const existing = claims.find(c => c.type === claimType)

        if (existing && existing.value === nodeId.value) continue // already up to date

        if (existing) {
          await identityStore.updateClaimAsync(existing.id, nodeId.value)
        } else {
          await identityStore.addClaimAsync(identity.publicKey, claimType, nodeId.value)
        }
      } catch (e) {
        console.warn(`[P2P] Failed to update device claim for identity:`, e)
      }
    }
  }

  const stopAsync = async () => {
    await invoke('peer_storage_stop')
    running.value = false
  }

  // =========================================================================
  // Remote peer operations
  // =========================================================================

  const remoteListAsync = async (remoteNodeId: string, path: string) => {
    return invoke<FileEntry[]>('peer_storage_remote_list', { nodeId: remoteNodeId, path })
  }

  const remoteReadAsync = async (remoteNodeId: string, path: string) => {
    return invoke<string>('peer_storage_remote_read', { nodeId: remoteNodeId, path })
  }

  return {
    running,
    nodeId,
    shares,
    spaceDevices,
    refreshStatusAsync,
    loadSharesAsync,
    loadSpaceDevicesAsync,
    startAsync,
    stopAsync,
    addShareAsync,
    removeShareAsync,
    registerDeviceInSpaceAsync,
    unregisterDeviceFromSpaceAsync,
    remoteListAsync,
    remoteReadAsync,
  }
})
