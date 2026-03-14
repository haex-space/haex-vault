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

  const registerDeviceInSpaceAsync = async (spaceId: string, deviceName: string) => {
    const db = currentVault.value?.drizzle
    if (!db) throw new Error('No vault open')
    if (!nodeId.value) throw new Error('Endpoint ID not available')

    await db.insert(haexSpaceDevices).values({
      spaceId,
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
