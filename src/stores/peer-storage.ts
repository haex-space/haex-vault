import { invoke, Channel } from '@tauri-apps/api/core'
import { RustEventGroup, RUST_EVENTS, type PeerStorageStateEvent } from '@/lib/rust-events'
import { eq } from 'drizzle-orm'
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
  haexVaultSettings,
  type SelectHaexPeerShares,
  type SelectHaexSpaceDevices,
} from '~/database/schemas'
import { VaultSettingsKeyEnum } from '~/config/vault-settings'
import { getUcanForSpaceAsync } from '~/utils/auth/ucanStore'
import { decodeUcan, type Capability } from '@haex-space/ucan'

const log = createLogger('PEER_STORAGE')

export const usePeerStorageStore = defineStore('peerStorageStore', () => {
  const running = ref(false)
  const nodeId = ref('')
  const relayUrl = ref<string | null>(null)
  const configuredRelayUrl = ref<string | null>(null)
  const shares = ref<SelectHaexPeerShares[]>([])
  const spaceDevices = ref<SelectHaexSpaceDevices[]>([])

  let stateEvents: RustEventGroup | null = null

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

  const loadSharesAsync = async (trigger: string = 'unknown') => {
    const db = requireDb()
    const before = shares.value.length
    shares.value = await db.select().from(haexPeerShares).all()
    log.warn(`UCAN-DIAG loadSharesAsync trigger=${trigger} before=${before} after=${shares.value.length}`)
  }

  const loadSpaceDevicesAsync = async (trigger: string = 'unknown') => {
    const db = requireDb()
    const before = spaceDevices.value.length
    spaceDevices.value = await db.select().from(haexSpaceDevices).all()
    log.warn(`UCAN-DIAG loadSpaceDevicesAsync trigger=${trigger} before=${before} after=${spaceDevices.value.length}`)
  }

  const addShareAsync = async (spaceId: string, name: string, localPath: string) => {
    const db = requireDb()
    const deviceStore = useDeviceStore()
    if (!deviceStore.deviceRowId || !deviceStore.deviceId) {
      throw new Error('Device identity not resolved — cannot add share')
    }

    await db.insert(haexPeerShares).values({
      spaceId,
      deviceId: deviceStore.deviceRowId,
      endpointId: deviceStore.deviceId,
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
  // Space device registration — explicit publishing, no auto-register
  // =========================================================================

  /**
   * Publish this device in a space. Called explicitly from the
   * Space-Publishing dialog or the "Geräte & Spaces" matrix settings page —
   * never automatically.
   */
  const registerDeviceInSpaceAsync = async (
    spaceId: string,
    nameOverride?: string,
    identityIdParam?: string,
  ) => {
    const db = requireDb()
    const deviceStore = useDeviceStore()
    if (!deviceStore.deviceRowId || !deviceStore.deviceId) {
      throw new Error('Device identity not resolved — cannot publish in space')
    }

    let identityId = identityIdParam
    if (!identityId) {
      const identityStore = useIdentityStore()
      identityId = identityStore.ownIdentities[0]?.id
    }

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

    const displayName = nameOverride
      || deviceStore.deviceName
      || deviceStore.hostname
      || `Device ${deviceStore.deviceId.slice(0, 8)}`

    await db.insert(haexSpaceDevices).values({
      spaceId,
      identityId: identityId || null,
      deviceId: deviceStore.deviceRowId,
      endpointId: deviceStore.deviceId,
      name: displayName,
      platform: deviceStore.platform,
      relayUrl: relayUrl.value,
    })

    await loadSpaceDevicesAsync()
  }

  const unregisterDeviceFromSpaceAsync = async (rowId: string) => {
    const db = requireDb()
    await db.delete(haexSpaceDevices).where(eq(haexSpaceDevices.id, rowId))
    await loadSpaceDevicesAsync()
  }

  // =========================================================================
  // Endpoint control
  // =========================================================================

  const startAsync = async () => {
    const deviceStore = useDeviceStore()
    if (!deviceStore.deviceRowId) {
      throw new Error(
        'Device identity not resolved yet — call useDeviceStore().resolveAsync() before starting P2P',
      )
    }

    // Make sure the iroh endpoint runs with the device's persistent secret
    // key, not the ephemeral one PeerEndpoint::new_ephemeral created.
    await deviceStore.loadEndpointKeyAsync()

    await loadConfiguredRelayUrlAsync()
    const info = await invoke<PeerStorageStartInfo>('peer_storage_start', {
      relayUrl: configuredRelayUrl.value || null,
    })
    running.value = true
    nodeId.value = info.nodeId
    relayUrl.value = info.relayUrl

    await loadSpaceDevicesAsync()
    if (relayUrl.value) {
      const db = requireDb()
      // Refresh the relay URL on our publish rows so peers see the current
      // one. We match by the random device row id (FK on haex_devices.id),
      // not by endpoint id, because endpoint id changes on reclaim.
      await db
        .update(haexSpaceDevices)
        .set({ relayUrl: relayUrl.value })
        .where(eq(haexSpaceDevices.deviceId, deviceStore.deviceRowId))
    }

    // Start leader mode for local spaces now that the P2P endpoint is active
    const spacesStore = useSpacesStore()
    await spacesStore.startLocalSpaceLeadersAsync()

    // For spaces where another device is the elected leader, start a peer
    // sync loop so we pull CRDT history.
    await spacesStore.startLocalSpacePeerSyncAsync()

    // Start enabled file sync rules
    const fileSyncStore = useFileSyncStore()
    await fileSyncStore.loadRulesAsync()
    await fileSyncStore.startEnabledRulesAsync()

    // Listen for Rust-side endpoint state changes. When Android suspends the
    // process, iroh closes the endpoint and emits this event. We restart the
    // full P2P stack so the user doesn't have to relaunch the app.
    stateEvents = new RustEventGroup()
    await stateEvents.on<PeerStorageStateEvent>(
      RUST_EVENTS.peerStorageStateChanged,
      ({ running: isRunning, reason, uptimeSecs }) => {
        if (!isRunning && running.value) {
          log.warn(`[P2P] Endpoint closed (reason=${reason}, uptime=${uptimeSecs}s), restarting`)
          running.value = false
          startAsync().catch(err => log.error('[P2P] Post-close restart failed:', err))
        }
      },
    )
  }

  const stopAsync = async () => {
    stateEvents?.dispose()
    stateEvents = null

    try {
      await invoke('file_sync_stop_all')
    } catch { /* ok if no syncs running */ }

    await invoke('peer_storage_stop')
    running.value = false
  }

  const restartAfterResumeAsync = async () => {
    if (!running.value) return
    log.info('[P2P-RESUME] Restarting P2P endpoint after app resume')
    try { await stopAsync() } catch { /* best-effort */ }
    await startAsync()
  }

  // =========================================================================
  // Remote peer operations
  // =========================================================================

  const activeTransfers = ref(0)
  const isTransferring = computed(() => activeTransfers.value > 0)

  interface TransferProgress {
    transferId: string
    path: string
    fileName: string
    bytesReceived: number
    totalBytes: number
    progress: number // 0-1
  }

  const transfers = ref<Map<string, TransferProgress>>(new Map())

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

  // Resolve which space a remote request belongs to, so the matching UCAN
  // can be picked. The first path segment is the share name; the share row
  // (replicated via CRDT) carries the authoritative spaceId.
  const resolveRequestContext = (remoteNodeId: string, path: string) => {
    const trimmed = path.replace(/^\/+/, '')
    const shareName = trimmed.split('/')[0]
    const matchingShare = shareName
      ? shares.value.find(
          s => s.endpointId === remoteNodeId && s.name === shareName,
        )
      : undefined
    const nodeShort = remoteNodeId.slice(0, 12)
    if (shareName && !matchingShare) {
      // Surface which subset of shares.value we *do* see for this peer so
      // the failure mode is unambiguous: stale cache (no rows at all) vs.
      // endpoint mismatch (rows present but different endpointId) vs.
      // name mismatch (right endpoint, wrong share name).
      const peerShares = shares.value
        .filter(s => s.endpointId === remoteNodeId)
        .map(s => s.name)
      log.warn(
        `UCAN-DIAG resolveRequestContext outcome=matchingShare-undefined node=${nodeShort} path=${path} shareName=${shareName} sharesTotal=${shares.value.length} sharesForPeer=${peerShares.length} peerShareNames=${JSON.stringify(peerShares)}`,
      )
      return { ucanToken: null, relayUrl: null }
    }
    const device = spaceDevices.value.find(
      d => d.endpointId === remoteNodeId
        && (matchingShare ? d.spaceId === matchingShare.spaceId : true),
    )
    const spaceId = matchingShare?.spaceId ?? device?.spaceId
    const ucanToken = spaceId ? getUcanForSpaceAsync(spaceId) : null
    const outcome = !ucanToken
      ? (spaceId ? 'ucan-cache-miss' : 'spaceId-undefined')
      : 'ok'
    if (!ucanToken || shareName) {
      // Non-root resolves and any failure: log full context. Root happy-path
      // (no shareName, ucanToken present) stays silent to avoid spam.
      // Use warn so DEFAULT_LOG_LEVEL='warn' in haex-vault doesn't filter
      // these out — these are diagnostic, not noise.
      log.warn(
        `UCAN-DIAG resolveRequestContext outcome=${outcome} node=${nodeShort} path=${path} shareName=${shareName || '(root)'} spaceId=${spaceId?.slice(0, 8) ?? 'none'} matchingShare=${!!matchingShare} device=${!!device} sharesTotal=${shares.value.length} devicesTotal=${spaceDevices.value.length}`,
      )
    }
    return { ucanToken, relayUrl: device?.relayUrl ?? null }
  }

  const getCapabilityForPeer = (
    remoteNodeId: string,
    path: string,
  ): Capability | null => {
    const { ucanToken } = resolveRequestContext(remoteNodeId, path)
    if (!ucanToken) return null
    try {
      const decoded = decodeUcan(ucanToken)
      const caps = decoded.payload.cap as Record<string, Capability>
      return Object.values(caps)[0] ?? null
    } catch {
      return null
    }
  }

  const remoteListAsync = async (remoteNodeId: string, path: string) => {
    const { ucanToken, relayUrl: deviceRelayUrl } = resolveRequestContext(remoteNodeId, path)
    if (!ucanToken) throw new Error('No valid UCAN token for this peer\'s space')
    activeTransfers.value++
    try {
      return await invoke<FileEntry[]>('peer_storage_remote_list', {
        nodeId: remoteNodeId,
        relayUrl: deviceRelayUrl,
        path,
        ucanToken,
      })
    } finally {
      activeTransfers.value--
    }
  }

  const remoteReadAsync = async (remoteNodeId: string, path: string, saveTo?: string) => {
    const { ucanToken, relayUrl: deviceRelayUrl } = resolveRequestContext(remoteNodeId, path)
    if (!ucanToken) throw new Error('No valid UCAN token for this peer\'s space')
    const transferId = crypto.randomUUID()
    const { channel, promise } = createTransferChannel(transferId, path)

    activeTransfers.value++
    try {
      await invoke<string>('peer_storage_remote_read', {
        nodeId: remoteNodeId,
        relayUrl: deviceRelayUrl,
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

  const remoteWriteAsync = async (
    remoteNodeId: string,
    remotePath: string,
    sourcePath: string,
  ) => {
    const { ucanToken, relayUrl: deviceRelayUrl } = resolveRequestContext(remoteNodeId, remotePath)
    if (!ucanToken) throw new Error('No valid UCAN token for this peer\'s space')
    activeTransfers.value++
    try {
      await invoke('peer_storage_remote_write', {
        nodeId: remoteNodeId,
        relayUrl: deviceRelayUrl,
        path: remotePath,
        sourcePath,
        ucanToken,
      })
    } finally {
      activeTransfers.value--
    }
  }

  const remoteCreateDirectoryAsync = async (
    remoteNodeId: string,
    remotePath: string,
  ) => {
    const { ucanToken, relayUrl: deviceRelayUrl } = resolveRequestContext(remoteNodeId, remotePath)
    if (!ucanToken) throw new Error('No valid UCAN token for this peer\'s space')
    await invoke('peer_storage_remote_create_directory', {
      nodeId: remoteNodeId,
      relayUrl: deviceRelayUrl,
      path: remotePath,
      ucanToken,
    })
  }

  const checkPeerOnlineAsync = async (remoteNodeId: string): Promise<boolean> => {
    try {
      const { ucanToken, relayUrl: deviceRelayUrl } = resolveRequestContext(remoteNodeId, '/')
      if (!ucanToken) return false
      await invoke<FileEntry[]>('peer_storage_remote_list', {
        nodeId: remoteNodeId,
        relayUrl: deviceRelayUrl,
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
    if (subPath === '/' || !subPath) return localPath
    if (isContentUri(subPath)) return subPath
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
    restartAfterResumeAsync,
    addShareAsync,
    removeShareAsync,
    registerDeviceInSpaceAsync,
    unregisterDeviceFromSpaceAsync,
    resolveRequestContext,
    remoteListAsync,
    remoteReadAsync,
    remoteWriteAsync,
    remoteCreateDirectoryAsync,
    getCapabilityForPeer,
    checkPeerOnlineAsync,
    localListAsync,
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
