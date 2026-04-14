import { invoke, Channel } from '@tauri-apps/api/core'
import { and, eq, inArray } from 'drizzle-orm'
import { createLogger } from '@/stores/logging'
import { requireDb } from '~/stores/vault'
import type { PeerStorageStatus } from '~/../src-tauri/bindings/PeerStorageStatus'
import type { PeerStorageStartInfo } from '~/../src-tauri/bindings/PeerStorageStartInfo'
import type { FileEntry } from '~/../src-tauri/bindings/FileEntry'
import type { DirEntry } from '~/../src-tauri/bindings/DirEntry'
import {
  haexIdentities,
  haexPeerShares,
  haexSpaceDevices,
  haexSpaceMembers,
  haexVaultSettings,
  type SelectHaexPeerShares,
  type SelectHaexSpaceDevices,
} from '~/database/schemas'
import { VaultSettingsKeyEnum } from '~/config/vault-settings'
import { getUcanForSpaceAsync } from '~/utils/auth/ucanStore'

const log = createLogger('PEER_STORAGE')

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
      log.error('Failed to get status:', error)
    }
  }

  // =========================================================================
  // DB-backed share management (via Drizzle / CRDT)
  // =========================================================================

  const loadConfiguredRelayUrlAsync = async () => {
    const db = requireDb()
    const row = await db.query.haexVaultSettings.findFirst({
      where: eq(haexVaultSettings.key, VaultSettingsKeyEnum.peerStorageRelayUrl),
    })
    configuredRelayUrl.value = row?.value || null
  }

  const saveConfiguredRelayUrlAsync = async (url: string | null) => {
    const db = requireDb()

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
        value: url,
      })
    }
    configuredRelayUrl.value = url
  }

  const loadSharesAsync = async () => {
    const db = requireDb()
    shares.value = await db.select().from(haexPeerShares).all()
  }

  const loadSpaceDevicesAsync = async () => {
    const db = requireDb()
    spaceDevices.value = await db.select().from(haexSpaceDevices).all()
  }

  const addShareAsync = async (spaceId: string, name: string, localPath: string) => {
    const db = requireDb()
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
    const db = requireDb()

    await db.delete(haexPeerShares).where(eq(haexPeerShares.id, shareId))

    await loadSharesAsync()
    await invoke('peer_storage_reload_shares')
  }

  // =========================================================================
  // Space device registration
  // =========================================================================

  const registerDeviceInSpaceAsync = async (spaceId: string, deviceName: string, identityIdParam?: string) => {
    const db = requireDb()
    if (!nodeId.value) throw new Error('Endpoint ID not available')

    // Resolve identity: use provided or first available
    let identityId = identityIdParam
    if (!identityId) {
      const identityStore = useIdentityStore()
      identityId = identityStore.ownIdentities[0]?.id
    }

    // Verify identity exists in DB before inserting (may not be synced yet)
    if (identityId) {
      const [identityExists] = await db
        .select({ id: haexIdentities.id })
        .from(haexIdentities)
        .where(eq(haexIdentities.id, identityId))
        .limit(1)
      if (!identityExists) {
        log.warn(`Identity ${identityId.substring(0, 8)}... not in DB yet, registering without identity`)
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

  const resolveOwnIdentityForSpaceAsync = async (spaceId: string, ownerIdentityId: string) => {
    const identityStore = useIdentityStore()
    await identityStore.loadIdentitiesAsync()

    const ownIdentityIds = identityStore.ownIdentities.map(i => i.id)
    if (ownIdentityIds.length === 0) return undefined
    if (ownIdentityIds.includes(ownerIdentityId)) return ownerIdentityId

    const db = requireDb()
    const [membership] = await db
      .select({ identityId: haexSpaceMembers.identityId })
      .from(haexSpaceMembers)
      .where(and(
        eq(haexSpaceMembers.spaceId, spaceId),
        inArray(haexSpaceMembers.identityId, ownIdentityIds),
      ))
      .limit(1)

    return membership?.identityId ?? ownIdentityIds[0]
  }

  const unregisterDeviceFromSpaceAsync = async (deviceId: string) => {
    const db = requireDb()

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
    if (relayUrl.value) {
      const db = requireDb()
      await db
        .update(haexSpaceDevices)
        .set({ relayUrl: relayUrl.value })
        .where(eq(haexSpaceDevices.deviceEndpointId, nodeId.value))
    }

    await autoRegisterInSpacesAsync()

    // Start leader mode for local spaces now that the P2P endpoint is active
    const spacesStore = useSpacesStore()
    await spacesStore.startLocalSpaceLeadersAsync()

    // Start enabled file sync rules
    const fileSyncStore = useFileSyncStore()
    await fileSyncStore.loadRulesAsync()
    await fileSyncStore.startEnabledRulesAsync()
  }

  const autoRegisterInSpacesAsync = async () => {
    if (!nodeId.value) return
    const db = requireDb()

    const spacesStore = useSpacesStore()
    const deviceStore = useDeviceStore()
    const hostname = deviceStore.deviceName || deviceStore.hostname || 'Unknown'

    for (const space of spacesStore.visibleSpaces) {
      const identityId = await resolveOwnIdentityForSpaceAsync(
        space.id,
        space.ownerIdentityId,
      )

      // Check if already registered with current endpoint ID
      const existingById = spaceDevices.value.find(
        d => d.spaceId === space.id && d.deviceEndpointId === nodeId.value,
      )
      if (existingById) {
        if (identityId && existingById.identityId !== identityId) {
          await db
            .update(haexSpaceDevices)
            .set({ identityId, relayUrl: relayUrl.value })
            .where(eq(haexSpaceDevices.id, existingById.id))
          await loadSpaceDevicesAsync()
        }
        continue
      }

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
            .set({
              deviceEndpointId: nodeId.value,
              identityId: identityId || staleEntry.identityId,
              relayUrl: relayUrl.value,
            })
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
          await registerDeviceInSpaceAsync(space.id, hostname, identityId)
        }
      } catch (e) {
        log.warn(`Failed to register in space ${space.id}:`, e)
      }
    }
  }

  const stopAsync = async () => {
    // Stop all active sync rules before shutting down P2P endpoint
    try {
      await invoke('file_sync_stop_all')
    } catch { /* ok if no syncs running */ }

    await invoke('peer_storage_stop')
    running.value = false
  }

  // =========================================================================
  // Remote peer operations
  // =========================================================================

  const activeTransfers = ref(0)
  const isTransferring = computed(() => activeTransfers.value > 0)

  // =========================================================================
  // Transfer progress tracking
  // =========================================================================

  interface TransferProgress {
    transferId: string
    path: string
    fileName: string
    bytesReceived: number
    totalBytes: number
    progress: number // 0-1
  }

  const transfers = ref<Map<string, TransferProgress>>(new Map())

  /** Create a Channel that streams transfer events from the Rust side. */
  const createTransferChannel = (transferId: string, path: string) => {
    type TransferEvent =
      | { event: 'progress'; bytesReceived: number; totalBytes: number }
      | { event: 'complete'; localPath: string; totalBytes: number }
      | { event: 'error'; error: string }

    let resolveTransfer: ((localPath: string) => void) | undefined
    let rejectTransfer: ((error: Error) => void) | undefined
    const fileName = path.split('/').pop() || path

    const promise = new Promise<string>((resolve, reject) => {
      resolveTransfer = resolve
      rejectTransfer = reject
    })

    const channel = new Channel<TransferEvent>()
    channel.onmessage = (msg) => {
      switch (msg.event) {
        case 'progress':
          transfers.value.set(transferId, {
            transferId,
            path,
            fileName,
            bytesReceived: msg.bytesReceived,
            totalBytes: msg.totalBytes,
            progress: msg.totalBytes > 0 ? msg.bytesReceived / msg.totalBytes : 0,
          })
          transfers.value = new Map(transfers.value)
          break
        case 'complete': {
          const transfer = transfers.value.get(transferId)
          if (transfer) {
            transfer.progress = 1
            transfers.value = new Map(transfers.value)
            setTimeout(() => {
              transfers.value.delete(transferId)
              transfers.value = new Map(transfers.value)
            }, 1500)
          }
          resolveTransfer?.(msg.localPath)
          break
        }
        case 'error':
          transfers.value.delete(transferId)
          transfers.value = new Map(transfers.value)
          rejectTransfer?.(new Error(msg.error))
          break
      }
    }

    return { channel, promise }
  }

  /** Get transfer progress for a specific file path (0-1, or undefined if not downloading) */
  const getTransferProgress = (filePath: string): number | undefined => {
    for (const t of transfers.value.values()) {
      if (t.path === filePath) return t.progress
    }
    return undefined
  }

  const activeDownloads = computed(() => Array.from(transfers.value.values()))

  const cancelTransferAsync = async (transferId: string) => {
    await invoke('peer_storage_transfer_cancel', { transferId })
    transfers.value.delete(transferId)
    transfers.value = new Map(transfers.value)
  }

  const pauseTransferAsync = async (transferId: string) => {
    await invoke('peer_storage_transfer_pause', { transferId })
  }

  const resumeTransferAsync = async (transferId: string) => {
    await invoke('peer_storage_transfer_resume', { transferId })
  }

  const remoteListAsync = async (remoteNodeId: string, path: string) => {
    const device = spaceDevices.value.find(d => d.deviceEndpointId === remoteNodeId)
    const ucanToken = device?.spaceId ? getUcanForSpaceAsync(device.spaceId) : null
    if (!ucanToken) throw new Error('No valid UCAN token for this peer\'s space')
    activeTransfers.value++
    try {
      return await invoke<FileEntry[]>('peer_storage_remote_list', {
        nodeId: remoteNodeId,
        relayUrl: device?.relayUrl ?? null,
        path,
        ucanToken,
      })
    } finally {
      activeTransfers.value--
    }
  }

  /** Download a remote file to disk. Returns the local file path once the download completes. */
  const remoteReadAsync = async (remoteNodeId: string, path: string, saveTo?: string) => {
    const device = spaceDevices.value.find(d => d.deviceEndpointId === remoteNodeId)
    const ucanToken = device?.spaceId ? getUcanForSpaceAsync(device.spaceId) : null
    if (!ucanToken) throw new Error('No valid UCAN token for this peer\'s space')
    const transferId = crypto.randomUUID()
    const { channel, promise } = createTransferChannel(transferId, path)

    activeTransfers.value++
    try {
      await invoke<string>('peer_storage_remote_read', {
        nodeId: remoteNodeId,
        relayUrl: device?.relayUrl ?? null,
        path,
        transferId,
        saveTo: saveTo ?? null,
        ucanToken,
        onEvent: channel,
      })

      return await promise
    } finally {
      activeTransfers.value--
    }
  }

  /** Check if a remote peer is reachable (lightweight root listing) */
  const checkPeerOnlineAsync = async (remoteNodeId: string): Promise<boolean> => {
    try {
      const device = spaceDevices.value.find(d => d.deviceEndpointId === remoteNodeId)
      const ucanToken = device?.spaceId ? getUcanForSpaceAsync(device.spaceId) : null
      if (!ucanToken) return false
      await invoke<FileEntry[]>('peer_storage_remote_list', {
        nodeId: remoteNodeId,
        relayUrl: device?.relayUrl ?? null,
        path: '/',
        ucanToken,
      })
      return true
    } catch {
      return false
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
    checkPeerOnlineAsync,
    localListAsync,
    // Transfer progress
    transfers,
    activeDownloads,
    getTransferProgress,
    cancelTransferAsync,
    pauseTransferAsync,
    resumeTransferAsync,
    reset: () => {
      running.value = false
      nodeId.value = ''
      relayUrl.value = null
      configuredRelayUrl.value = null
      shares.value = []
      spaceDevices.value = []
      transfers.value.clear()
    },
  }
})
