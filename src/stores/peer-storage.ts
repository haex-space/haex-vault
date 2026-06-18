import { invoke, Channel } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { RUST_EVENTS, type PeerStorageStateEvent } from '@/lib/rust-events'
import { createOnceListener, type OnceListener } from '@/lib/once-listener'
import { and, eq, or } from 'drizzle-orm'
import { createLogger } from '@/stores/logging'
import { requireDb } from '~/stores/vault'
import type { PeerStorageStatus } from '~/../src-tauri/bindings/PeerStorageStatus'
import type { PeerStorageStartInfo } from '~/../src-tauri/bindings/PeerStorageStartInfo'
import type { FileEntry } from '~/../src-tauri/bindings/FileEntry'
import type { DirEntry } from '~/../src-tauri/bindings/DirEntry'
import {
  haexIdentities,
  haexPeerShares,
  haexPendingInvites,
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
  // (spaceId, endpointId) tuples extracted from accepted invites whose
  // haex_space_devices row has not yet arrived via CRDT sync. Used as a
  // fallback in `resolveRequestContext` so the file-browser-root resolver
  // can map an inviter's endpoint to its space immediately after accept,
  // closing the race window between accept-complete and CRDT-row-arrived.
  const acceptedInviteEndpoints = ref<Array<{ spaceId: string, endpointId: string }>>([])

  let stateEvents: OnceListener | null = null
  // Guards against concurrent/duplicate starts. The endpoint can now be
  // triggered from several places (mount-time gate, the deviceRowId watcher
  // in vault.vue, the outbox processor, the post-close restart handler); two
  // calls racing before `running` flips to true would otherwise both reach
  // peer_storage_start and hit its EndpointAlreadyRunning assert.
  //
  // Holding an in-flight promise (rather than a bool) lets concurrent callers
  // await the same startup and observe the same outcome — `nodeId` is set,
  // or the start error propagates to every caller. The outbox processor reads
  // `nodeId` right after `await startAsync()`, so a boolean flag returning
  // early would let it proceed against an empty endpoint.
  let startingPromise: Promise<void> | null = null

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

  const loadAcceptedInviteEndpointsAsync = async () => {
    const db = requireDb()
    const rows = await db
      .select({
        spaceId: haexPendingInvites.spaceId,
        spaceEndpoints: haexPendingInvites.spaceEndpoints,
      })
      .from(haexPendingInvites)
      .where(eq(haexPendingInvites.status, 'accepted'))
      .all()
    const tuples: Array<{ spaceId: string, endpointId: string }> = []
    for (const row of rows) {
      if (!row.spaceEndpoints) continue
      try {
        const endpoints = JSON.parse(row.spaceEndpoints) as unknown
        if (!Array.isArray(endpoints)) continue
        for (const endpointId of endpoints) {
          if (typeof endpointId === 'string' && endpointId.length > 0) {
            tuples.push({ spaceId: row.spaceId, endpointId })
          }
        }
      } catch {
        // Malformed JSON — skip this invite, don't fail the whole load.
      }
    }
    acceptedInviteEndpoints.value = tuples
  }

  const addShareAsync = async (spaceId: string, name: string, localPath: string) => {
    const db = requireDb()
    const deviceStore = useDeviceStore()
    if (!deviceStore.deviceRowId || !deviceStore.deviceId) {
      throw new Error('Device identity not resolved — cannot add share')
    }

    // Ensure this device is published in the space before adding the share.
    // Without the haex_space_devices row, peers receive the peer_shares row via
    // CRDT sync but cannot resolve the device — `allowed_peers` stays empty and
    // sub-folder listings fail on the auth check. registerDeviceInSpaceAsync is
    // idempotent (upserts on (space_id, endpoint_id)), so calling it on every
    // share-add is safe.
    await registerDeviceInSpaceAsync(spaceId)

    // Self-attribute the row. SyncPush re-injects authored_by_did from the
    // validated UCAN audience, but SyncPull serves rows raw — so a peer
    // pulling the leader's local row would otherwise see NULL, which also
    // disables the haex_peer_shares_ensure_refs trigger and leaves device_id
    // dangling. See validate.rs:52-87 and 0001_late_spyke.sql:130-146.
    //
    // Hydrate the identity store before reading: in some flows
    // (Tauri-restored sessions, freshly-opened vault) the store hasn't
    // loaded yet, and an unhydrated read returns NULL which would
    // reintroduce the exact failure mode the attribution fix is meant to
    // close. loadIdentitiesAsync is idempotent and cheap on cache hit.
    const identityStore = useIdentityStore()
    if (identityStore.ownIdentities.length === 0) {
      await identityStore.loadIdentitiesAsync()
    }
    const authoredByDid = identityStore.ownIdentities[0]?.did ?? null

    await db.insert(haexPeerShares).values({
      spaceId,
      deviceId: deviceStore.deviceRowId,
      endpointId: deviceStore.deviceId,
      name,
      localPath,
      authoredByDid,
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

    // Hydrate the identity store before deriving `identityId` /
    // `authoredByDid` from it — see the matching note in `addShareAsync`.
    // loadIdentitiesAsync is idempotent and cheap on cache hit.
    const identityStore = useIdentityStore()
    if (identityStore.ownIdentities.length === 0) {
      await identityStore.loadIdentitiesAsync()
    }
    let identityId = identityIdParam
    if (!identityId) {
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

    // Self-attribute the row so SyncPull peers see the author's DID instead
    // of NULL. SyncPush would re-inject this from the validated UCAN, but
    // pulls serve rows raw. See addShareAsync for the same rationale.
    const authoredByDid = identityId
      ? identityStore.identities.find(i => i.id === identityId)?.did ?? null
      : identityStore.ownIdentities[0]?.did ?? null

    const displayName = nameOverride
      || deviceStore.deviceName
      || deviceStore.hostname
      || `Device ${deviceStore.deviceId.slice(0, 8)}`

    // Idempotent publish: a previous membership (e.g. leave → re-invite)
    // leaves the haex_space_devices row behind because self-leave only
    // tears down haex_space_members. Re-publishing would otherwise hit a
    // UNIQUE constraint — the table has two: (space_id, endpoint_id) and
    // (space_id, device_id). After a reclaim the endpoint_id rotates but
    // device_id (= haex_devices.id) stays the same, so we have to look up
    // by either column.
    const existing = await db
      .select({ id: haexSpaceDevices.id })
      .from(haexSpaceDevices)
      .where(and(
        eq(haexSpaceDevices.spaceId, spaceId),
        or(
          eq(haexSpaceDevices.endpointId, deviceStore.deviceId),
          eq(haexSpaceDevices.deviceId, deviceStore.deviceRowId),
        ),
      ))
      .limit(1)

    if (existing[0]) {
      // Refresh endpoint_id alongside the rest: a reclaim leaves the row
      // pointing at the rotated-away public key, which would prevent peers
      // from authorising this device on the new endpoint.
      await db.update(haexSpaceDevices)
        .set({
          identityId: identityId || null,
          deviceId: deviceStore.deviceRowId,
          endpointId: deviceStore.deviceId,
          name: displayName,
          platform: deviceStore.platform,
          relayUrl: relayUrl.value,
          authoredByDid,
        })
        .where(eq(haexSpaceDevices.id, existing[0].id))
    } else {
      await db.insert(haexSpaceDevices).values({
        spaceId,
        identityId: identityId || null,
        deviceId: deviceStore.deviceRowId,
        endpointId: deviceStore.deviceId,
        name: displayName,
        platform: deviceStore.platform,
        relayUrl: relayUrl.value,
        authoredByDid,
      })
    }

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

    // Idempotent: a no-op when the endpoint is already up; concurrent callers
    // wait on the in-flight start so they observe the post-start state (and
    // surface any error) rather than returning before `running` is true.
    if (running.value) return
    if (startingPromise) {
      await startingPromise
      return
    }
    startingPromise = (async () => {
      try {
        await startEndpointAsync(deviceStore)
      } finally {
        startingPromise = null
      }
    })()
    await startingPromise
  }

  const startEndpointAsync = async (deviceStore: ReturnType<typeof useDeviceStore>) => {
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
    await loadAcceptedInviteEndpointsAsync()
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
    //
    // The handler re-enters `startAsync()` on close-event; gate creation so
    // we don't overwrite a live OnceListener instance (whose unlisten would
    // then be unreachable, leaving the Tauri-side listener leaked).
    if (!stateEvents) {
      stateEvents = createOnceListener(() =>
        listen<PeerStorageStateEvent>(
          RUST_EVENTS.peerStorageStateChanged,
          (event) => {
            const { running: isRunning, reason, uptimeSecs } = event.payload
            if (!isRunning && running.value) {
              log.warn(`[P2P] Endpoint closed (reason=${reason}, uptime=${uptimeSecs}s), restarting`)
              running.value = false
              startAsync().catch(err => log.error('[P2P] Post-close restart failed:', err))
            }
          },
          { target: 'main' },
        ),
      )
    }
    await stateEvents.initAsync()
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
    direction: 'download' | 'upload'
    bytesReceived: number
    totalBytes: number
    progress: number // 0-1
    startedAt: number // Date.now() at first progress tick
    bytesPerSec: number // EMA-smoothed throughput, alpha = 0.3
    paused: boolean
  }

  const transfers = ref<Map<string, TransferProgress>>(new Map())

  const createTransferChannel = (
    transferId: string,
    path: string,
    direction: 'download' | 'upload',
  ) => {
    type TransferEvent =
      | { event: 'progress'; bytesReceived: number; totalBytes: number }
      | { event: 'complete'; localPath: string; totalBytes: number }
      | { event: 'error'; error: string }

    let resolveTransfer: ((localPath: string) => void) | undefined
    let rejectTransfer: ((error: Error) => void) | undefined
    const fileName = path.split('/').pop() || path

    const startedAt = Date.now()
    let lastSampleAt = startedAt
    let lastBytes = 0
    // EMA so the displayed rate doesn't twitch on every 100 ms progress tick.
    // Seeded from the first instant rate so the chip is not stuck on 0 B/s
    // for the first ~1 s of a transfer.
    let smoothedBytesPerSec = 0

    const promise = new Promise<string>((resolve, reject) => {
      resolveTransfer = resolve
      rejectTransfer = reject
    })

    const channel = new Channel<TransferEvent>()
    channel.onmessage = (msg) => {
      switch (msg.event) {
        case 'progress': {
          // Snapshot the paused flag from the prior tick. A trailing chunk can
          // still arrive after pause (the backend cancels at the next chunk
          // boundary, not mid-chunk); skipping the EMA update there keeps the
          // displayed rate honest at 0 B/s for paused transfers.
          const paused = transfers.value.get(transferId)?.paused ?? false
          const now = Date.now()
          const dt = (now - lastSampleAt) / 1000
          if (!paused && dt > 0) {
            const instant = (msg.bytesReceived - lastBytes) / dt
            smoothedBytesPerSec
              = smoothedBytesPerSec === 0 ? instant : 0.3 * instant + 0.7 * smoothedBytesPerSec
          }
          lastSampleAt = now
          lastBytes = msg.bytesReceived

          transfers.value.set(transferId, {
            transferId,
            path,
            fileName,
            direction,
            bytesReceived: msg.bytesReceived,
            totalBytes: msg.totalBytes,
            progress: msg.totalBytes > 0 ? msg.bytesReceived / msg.totalBytes : 0,
            startedAt,
            bytesPerSec: paused ? 0 : smoothedBytesPerSec,
            paused,
          })
          transfers.value = new Map(transfers.value)
          break
        }
        case 'complete': {
          const transfer = transfers.value.get(transferId)
          if (transfer) {
            transfer.progress = 1
            // Zero the rate so the aggregate `totalBytesPerSec` chip doesn't
            // keep the just-completed transfer's last sample alive during the
            // 1.5 s linger window — otherwise the chip lies about being busy.
            transfer.bytesPerSec = 0
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

  const getTransferIdForPath = (filePath: string): string | undefined => {
    for (const t of transfers.value.values()) {
      if (t.path === filePath) return t.transferId
    }
    return undefined
  }

  const activeDownloads = computed(() => Array.from(transfers.value.values()))

  /**
   * Sum of EMA-smoothed throughput across all in-flight **downloads**.
   * Uploads share the same `transfers` map but the file-browser chip is
   * labelled as a download indicator — mixing both would make the number
   * lie when a user uploads and downloads simultaneously.
   */
  const totalBytesPerSec = computed(() => {
    let total = 0
    for (const t of transfers.value.values()) {
      if (t.direction === 'download') total += t.bytesPerSec
    }
    return total
  })

  const cancelTransferAsync = async (transferId: string) => {
    await invoke('peer_storage_transfer_cancel', { transferId })
    transfers.value.delete(transferId)
    transfers.value = new Map(transfers.value)
  }

  const setTransferPaused = (transferId: string, paused: boolean) => {
    const t = transfers.value.get(transferId)
    if (t) {
      t.paused = paused
      // Zero the throughput chip while paused so it doesn't keep showing the
      // pre-pause rate; it recovers from the next progress tick after resume.
      if (paused) t.bytesPerSec = 0
      transfers.value = new Map(transfers.value)
    }
  }

  const pauseTransferAsync = async (transferId: string) => {
    await invoke('peer_storage_transfer_pause', { transferId })
    setTransferPaused(transferId, true)
  }

  const resumeTransferAsync = async (transferId: string) => {
    await invoke('peer_storage_transfer_resume', { transferId })
    setTransferPaused(transferId, false)
  }

  const getTransferPaused = (filePath: string): boolean => {
    for (const t of transfers.value.values()) {
      if (t.path === filePath) return t.paused
    }
    return false
  }

  // Resolve which space a remote request belongs to, so the matching UCAN
  // can be picked. The first path segment is the share name; the share row
  // (replicated via CRDT) carries the authoritative spaceId.
  //
  // `spaceIdHint` is used when the caller already knows the authoritative
  // spaceId (e.g. an entry from `remoteListAllSharesAsync` that knows its
  // origin space). This bypasses the by-name lookup, which is otherwise
  // ambiguous when a single peer hosts shares with identical names in
  // different spaces. Without the hint we sort matching shares/devices by
  // spaceId so at least the picked space is stable across calls.
  //
  // Root-path lookups (path='/') depend on the inviter's haex_space_devices
  // row, which only lands after the CRDT pull following accept. Without a
  // fallback, clicking the file browser between accept-complete and
  // CRDT-row-arrived throws "No valid UCAN token". We can't seed
  // haex_space_devices ourselves: the synthetic deviceId we'd have to pass
  // (the invite payload doesn't carry the inviter's real one) fires
  // haex_space_devices_ensure_refs and creates a haex_devices stub claiming
  // the inviter's endpoint_id. That stub then blocks any later peer_shares
  // CRDT row (whose ensure-refs trigger silently fails the INSERT OR IGNORE
  // on haex_devices_endpoint_id_unique, leaving peer_shares.device_id
  // dangling). Instead, fall back to (spaceId, endpoint) tuples extracted
  // from accepted invites — same information, no schema corruption.
  const resolveRequestContext = (
    remoteNodeId: string,
    path: string,
    spaceIdHint?: string,
  ) => {
    const trimmed = path.replace(/^\/+/, '')
    const shareName = trimmed.split('/')[0]
    let matchingShare: SelectHaexPeerShares | undefined
    if (spaceIdHint && shareName) {
      matchingShare = shares.value.find(
        s => s.endpointId === remoteNodeId
          && s.name === shareName
          && s.spaceId === spaceIdHint,
      )
    } else if (shareName) {
      const candidates = shares.value
        .filter(s => s.endpointId === remoteNodeId && s.name === shareName)
        .sort((a, b) => a.spaceId.localeCompare(b.spaceId))
      matchingShare = candidates[0]
    }
    if (shareName && !matchingShare) {
      return { ucanToken: null, relayUrl: null }
    }
    const resolvedSpaceId = matchingShare?.spaceId ?? spaceIdHint
    const deviceCandidates = spaceDevices.value
      .filter(d => d.endpointId === remoteNodeId
        && (resolvedSpaceId ? d.spaceId === resolvedSpaceId : true))
      .sort((a, b) => a.spaceId.localeCompare(b.spaceId))
    const device = deviceCandidates[0]
    let spaceId = resolvedSpaceId ?? device?.spaceId
    if (!spaceId) {
      const inviteTuple = acceptedInviteEndpoints.value.find(
        t => t.endpointId === remoteNodeId,
      )
      spaceId = inviteTuple?.spaceId
    }
    const ucanToken = spaceId ? getUcanForSpaceAsync(spaceId) : null
    return { ucanToken, relayUrl: device?.relayUrl ?? null }
  }

  const getCapabilityForPeer = (
    remoteNodeId: string,
    path: string,
    spaceIdHint?: string,
  ): Capability | null => {
    const { ucanToken } = resolveRequestContext(remoteNodeId, path, spaceIdHint)
    if (!ucanToken) return null
    try {
      const decoded = decodeUcan(ucanToken)
      const caps = decoded.payload.cap as Record<string, Capability>
      return Object.values(caps)[0] ?? null
    } catch {
      return null
    }
  }

  const remoteListAsync = async (
    remoteNodeId: string,
    path: string,
    spaceIdHint?: string,
  ) => {
    const { ucanToken, relayUrl: deviceRelayUrl } = resolveRequestContext(
      remoteNodeId, path, spaceIdHint,
    )
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

  // Root listing across all shared spaces with a peer. `remoteListAsync` with
  // path='/' can only see ONE space's shares: the leader enforces a Layer-1.5
  // narrowing of effective_spaces = UCAN.capabilities ∩ allowed_peers, so the
  // returned set is filtered to the single space whose UCAN was sent. When a
  // peer shares multiple spaces with us, the file-browser-root view would
  // otherwise show only one space's shares (the one picked by the FIRST
  // device row match in resolveRequestContext, which is non-deterministic and
  // also leaks names across runs). This fans out one parallel request per
  // space we share with the peer, each scoped to that space's UCAN, and
  // tags each returned entry with its origin spaceId so downstream calls
  // can address the right space when share names collide across spaces.
  const remoteListAllSharesAsync = async (
    remoteNodeId: string,
  ): Promise<Array<FileEntry & { spaceId: string }>> => {
    const peerSpaceIds = [...new Set(
      spaceDevices.value
        .filter(d => d.endpointId === remoteNodeId)
        .map(d => d.spaceId),
    )]

    if (peerSpaceIds.length === 0) return []

    const fetchOneSpace = async (
      spaceId: string,
    ): Promise<Array<FileEntry & { spaceId: string }>> => {
      const ucanToken = getUcanForSpaceAsync(spaceId)
      if (!ucanToken) {
        log.warn(`remoteListAllSharesAsync: skipping space ${spaceId.slice(0, 8)} — no cached UCAN`)
        return []
      }
      const device = spaceDevices.value.find(
        d => d.endpointId === remoteNodeId && d.spaceId === spaceId,
      )
      activeTransfers.value++
      try {
        const entries = await invoke<FileEntry[]>('peer_storage_remote_list', {
          nodeId: remoteNodeId,
          relayUrl: device?.relayUrl ?? null,
          path: '/',
          ucanToken,
        })
        return entries.map(entry => ({ ...entry, spaceId }))
      } catch (err) {
        // Re-throw so the caller can surface this to the user if all spaces fail.
        throw new Error(`space ${spaceId.slice(0, 8)}: ${err}`)
      } finally {
        activeTransfers.value--
      }
    }

    const settled = await Promise.allSettled(peerSpaceIds.map(fetchOneSpace))
    const fulfilled = settled.filter(
      (r): r is PromiseFulfilledResult<Array<FileEntry & { spaceId: string }>> =>
        r.status === 'fulfilled',
    )
    const succeeded = fulfilled.flatMap(r => r.value)
    const failures = settled.filter((r): r is PromiseRejectedResult => r.status === 'rejected')

    if (fulfilled.length === 0 && failures.length > 0) {
      // Every attempted space failed to connect — throw the first connection
      // error so the file browser can show a meaningful message.
      throw failures[0]!.reason
    }

    return succeeded
  }

  const remoteReadAsync = async (
    remoteNodeId: string,
    path: string,
    saveTo?: string,
    spaceIdHint?: string,
    expectedSize?: number | bigint,
    expectedModified?: number | bigint | null,
    spaceFolder?: string | null,
  ) => {
    const { ucanToken, relayUrl: deviceRelayUrl } = resolveRequestContext(
      remoteNodeId, path, spaceIdHint,
    )
    if (!ucanToken) throw new Error('No valid UCAN token for this peer\'s space')
    const transferId = crypto.randomUUID()
    const { channel, promise } = createTransferChannel(transferId, path, 'download')

    activeTransfers.value++
    try {
      await invoke<string>('peer_storage_remote_read', {
        nodeId: remoteNodeId,
        relayUrl: deviceRelayUrl,
        path,
        transferId,
        saveTo: saveTo ?? null,
        expectedSize: expectedSize == null ? null : Number(expectedSize),
        expectedModified: expectedModified == null ? null : Number(expectedModified),
        spaceFolder: spaceFolder ?? null,
        spaceId: spaceIdHint ?? null,
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
    spaceIdHint?: string,
  ) => {
    const { ucanToken, relayUrl: deviceRelayUrl } = resolveRequestContext(
      remoteNodeId, remotePath, spaceIdHint,
    )
    if (!ucanToken) throw new Error('No valid UCAN token for this peer\'s space')

    const transferId = crypto.randomUUID()
    const { channel, promise } = createTransferChannel(transferId, remotePath, 'upload')

    activeTransfers.value++
    try {
      await invoke('peer_storage_remote_write', {
        nodeId: remoteNodeId,
        relayUrl: deviceRelayUrl,
        path: remotePath,
        sourcePath,
        transferId,
        ucanToken,
        onEvent: channel,
      })

      await promise
    } finally {
      activeTransfers.value--
    }
  }

  const remoteCreateDirectoryAsync = async (
    remoteNodeId: string,
    remotePath: string,
    spaceIdHint?: string,
  ) => {
    const { ucanToken, relayUrl: deviceRelayUrl } = resolveRequestContext(
      remoteNodeId, remotePath, spaceIdHint,
    )
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
    acceptedInviteEndpoints,
    refreshStatusAsync,
    loadSharesAsync,
    loadSpaceDevicesAsync,
    loadAcceptedInviteEndpointsAsync,
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
    remoteListAllSharesAsync,
    remoteReadAsync,
    remoteWriteAsync,
    remoteCreateDirectoryAsync,
    getCapabilityForPeer,
    checkPeerOnlineAsync,
    localListAsync,
    transfers,
    activeDownloads,
    totalBytesPerSec,
    getTransferProgress,
    getTransferIdForPath,
    getTransferPaused,
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
      acceptedInviteEndpoints.value = []
      transfers.value.clear()
    },
  }
})
