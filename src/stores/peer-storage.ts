import { invoke } from '@tauri-apps/api/core'
import { eq } from 'drizzle-orm'
import type { PeerStorageStatus } from '~/../src-tauri/bindings/PeerStorageStatus'
import type { PeerStorageStartInfo } from '~/../src-tauri/bindings/PeerStorageStartInfo'
import type { FileEntry } from '~/../src-tauri/bindings/FileEntry'
import type { DirEntry } from '~/../src-tauri/bindings/DirEntry'
import {
  haexIdentities,
  haexPeerShares,
  haexSpaceDevices,
  haexVaultSettings,
  type SelectHaexPeerShares,
  type SelectHaexSpaceDevices,
} from '~/database/schemas'
import { VaultSettingsKeyEnum, VaultSettingsTypeEnum } from '~/config/vault-settings'

export const usePeerStorageStore = defineStore('peerStorageStore', () => {
  const { currentVault } = storeToRefs(useVaultStore())

  const running = ref(false)
  const nodeId = ref('')
  const relayUrl = ref<string | null>(null)
  const configuredRelayUrl = ref<string | null>(null)
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

  const loadConfiguredRelayUrlAsync = async () => {
    const db = currentVault.value?.drizzle
    if (!db) return
    const row = await db.query.haexVaultSettings.findFirst({
      where: eq(haexVaultSettings.key, VaultSettingsKeyEnum.peerStorageRelayUrl),
    })
    configuredRelayUrl.value = row?.value || null
  }

  const saveConfiguredRelayUrlAsync = async (url: string | null) => {
    const db = currentVault.value?.drizzle
    if (!db) throw new Error('No vault open')

    const existing = await db.query.haexVaultSettings.findFirst({
      where: eq(haexVaultSettings.key, VaultSettingsKeyEnum.peerStorageRelayUrl),
    })

    if (existing) {
      if (url) {
        await db.update(haexVaultSettings)
          .set({ value: url })
          .where(eq(haexVaultSettings.key, VaultSettingsKeyEnum.peerStorageRelayUrl))
      } else {
        await db.delete(haexVaultSettings)
          .where(eq(haexVaultSettings.key, VaultSettingsKeyEnum.peerStorageRelayUrl))
      }
    } else if (url) {
      await db.insert(haexVaultSettings).values({
        id: crypto.randomUUID(),
        key: VaultSettingsKeyEnum.peerStorageRelayUrl,
        type: VaultSettingsTypeEnum.settings,
        value: url,
      })
    }
    configuredRelayUrl.value = url
  }

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

    // Verify identity exists in DB before inserting (may not be synced yet)
    if (identityId) {
      const [identityExists] = await db
        .select({ pk: haexIdentities.publicKey })
        .from(haexIdentities)
        .where(eq(haexIdentities.publicKey, identityId))
        .limit(1)
      if (!identityExists) {
        console.warn(`[P2P] Identity ${identityId.substring(0, 20)}... not in DB yet, registering without identity`)
        identityId = undefined
      }
    }

    await db.insert(haexSpaceDevices).values({
      spaceId,
      identityId: identityId || null,
      deviceEndpointId: nodeId.value,
      deviceName,
      relayUrl: relayUrl.value,
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
    await loadConfiguredRelayUrlAsync()
    const info = await invoke<PeerStorageStartInfo>('peer_storage_start', {
      relayUrl: configuredRelayUrl.value || null,
    })
    running.value = true
    nodeId.value = info.nodeId
    relayUrl.value = info.relayUrl

    // Load existing devices and update relay URL for our existing registrations
    await loadSpaceDevicesAsync()
    const db = currentVault.value?.drizzle
    if (db && relayUrl.value) {
      await db
        .update(haexSpaceDevices)
        .set({ relayUrl: relayUrl.value })
        .where(eq(haexSpaceDevices.deviceEndpointId, nodeId.value))
    }

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
      // Check if already registered with current endpoint ID
      const existingById = spaceDevices.value.find(
        d => d.spaceId === space.id && d.deviceEndpointId === nodeId.value,
      )
      if (existingById) continue

      // Check if this device was previously registered with a different endpoint ID
      // (happens when vault is deleted and reconnected → new device key → new endpoint ID)
      const staleEntry = spaceDevices.value.find(
        d => d.spaceId === space.id && d.deviceName === hostname && d.deviceEndpointId !== nodeId.value,
      )

      try {
        if (staleEntry) {
          // Update existing entry with new endpoint ID instead of creating a duplicate
          const oldEndpointId = staleEntry.deviceEndpointId
          await db
            .update(haexSpaceDevices)
            .set({ deviceEndpointId: nodeId.value, relayUrl: relayUrl.value })
            .where(eq(haexSpaceDevices.id, staleEntry.id))
          // Also migrate shares from old endpoint ID to new one
          await db
            .update(haexPeerShares)
            .set({ deviceEndpointId: nodeId.value })
            .where(eq(haexPeerShares.deviceEndpointId, oldEndpointId))
          await loadSpaceDevicesAsync()
          await loadSharesAsync()
          await invoke('peer_storage_reload_shares')
        } else {
          await registerDeviceInSpaceAsync(space.id, hostname)
        }
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

  const activeTransfers = ref(0)
  const isTransferring = computed(() => activeTransfers.value > 0)

  const remoteListAsync = async (remoteNodeId: string, path: string) => {
    const device = spaceDevices.value.find(d => d.deviceEndpointId === remoteNodeId)
    activeTransfers.value++
    try {
      return await invoke<FileEntry[]>('peer_storage_remote_list', {
        nodeId: remoteNodeId,
        relayUrl: device?.relayUrl ?? null,
        path,
      })
    } finally {
      activeTransfers.value--
    }
  }

  const remoteReadAsync = async (remoteNodeId: string, path: string) => {
    const device = spaceDevices.value.find(d => d.deviceEndpointId === remoteNodeId)
    activeTransfers.value++
    try {
      return await invoke<string>('peer_storage_remote_read', {
        nodeId: remoteNodeId,
        relayUrl: device?.relayUrl ?? null,
        path,
      })
    } finally {
      activeTransfers.value--
    }
  }

  const isContentUri = (p: string) => p.startsWith('{')

  const resolveLocalPath = (localPath: string, subPath: string) => {
    // Root level or no sub-path → use base path
    if (subPath === '/' || !subPath) return localPath
    // Android Content URI as subPath → already a full path from DirEntry
    if (isContentUri(subPath)) return subPath
    // Desktop: normal path concatenation
    return `${localPath}/${subPath.replace(/^\//, '')}`
  }

  const mapDirEntry = (e: DirEntry) => ({
    name: e.name,
    path: e.path,
    size: BigInt(e.size),
    isDir: e.isDirectory,
    modified: e.modified ? BigInt(e.modified) / 1000n : null,
  })

  const localListAsync = async (localPath: string, subPath: string, offset?: number, limit?: number) => {
    const target = resolveLocalPath(localPath, subPath)
    const result = await invoke<{ entries: DirEntry[]; total: number }>('filesystem_read_dir', {
      path: target,
      offset: offset ?? null,
      limit: limit ?? null,
    })
    return { entries: result.entries.map(mapDirEntry), total: result.total }
  }

  return {
    running,
    nodeId,
    relayUrl,
    configuredRelayUrl,
    isTransferring,
    shares,
    spaceDevices,
    refreshStatusAsync,
    loadSharesAsync,
    loadSpaceDevicesAsync,
    loadConfiguredRelayUrlAsync,
    saveConfiguredRelayUrlAsync,
    startAsync,
    stopAsync,
    addShareAsync,
    removeShareAsync,
    registerDeviceInSpaceAsync,
    unregisterDeviceFromSpaceAsync,
    remoteListAsync,
    remoteReadAsync,
    localListAsync,
  }
})
