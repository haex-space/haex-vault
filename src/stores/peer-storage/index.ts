import type {
  SelectHaexPeerShares,
  SelectHaexSpaceDevices,
} from '~/database/schemas'
import { createLogger } from '@/stores/logging'
import {
  startOwnerSyncAsync as invokeStartOwnerSync,
  stopOwnerSyncAsync as invokeStopOwnerSync,
  forceOwnerSyncAsync as invokeForceOwnerSync,
} from './owner-sync'
import { createSharesModule } from './shares'
import { createTransfersModule, localListAsync, type TransferProgress } from './transfers'
import { createPeersModule } from './peers'
import { createLifecycleModule } from './lifecycle'

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
  // Whether owner-device sync (serverless P2P sync of the owner's own vault
  // across the owner's other devices) is currently running. Driven by the
  // endpoint lifecycle: set true on autostart, false on stop/close.
  const ownerSyncRunning = ref(false)

  const activeTransfers = ref(0)
  const isTransferring = computed(() => activeTransfers.value > 0)
  const transfers = ref<Map<string, TransferProgress>>(new Map())

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

  // ---------------------------------------------------------------------
  // Owner-device sync control
  // ---------------------------------------------------------------------

  const startOwnerSyncAsync = async () => {
    try {
      await invokeStartOwnerSync()
      ownerSyncRunning.value = true
    } catch (error) {
      log.error('[owner-sync] start failed:', error)
      throw error
    }
  }

  const stopOwnerSyncAsync = async () => {
    try {
      await invokeStopOwnerSync()
    } finally {
      ownerSyncRunning.value = false
    }
  }

  const forceOwnerSyncAsync = async () => {
    await invokeForceOwnerSync()
  }

  // ---------------------------------------------------------------------
  // Sibling modules wired with the local refs
  // ---------------------------------------------------------------------

  const sharesModule = createSharesModule({
    shares,
    spaceDevices,
    acceptedInviteEndpoints,
    configuredRelayUrl,
    relayUrl,
  })

  const transfersModule = createTransfersModule({
    transfers,
    activeTransfers,
  })

  const peersModule = createPeersModule({
    shares,
    spaceDevices,
    acceptedInviteEndpoints,
    activeTransfers,
    createTransferChannel: transfersModule.createTransferChannel,
  })

  const lifecycleModule = createLifecycleModule({
    running,
    nodeId,
    relayUrl,
    configuredRelayUrl,
    ownerSyncRunning,
    loadConfiguredRelayUrlAsync: sharesModule.loadConfiguredRelayUrlAsync,
    loadSpaceDevicesAsync: sharesModule.loadSpaceDevicesAsync,
    loadAcceptedInviteEndpointsAsync: sharesModule.loadAcceptedInviteEndpointsAsync,
    startOwnerSyncAsync,
    stopOwnerSyncAsync,
    requestRestart: () => startAsync(),
  })

  // ---------------------------------------------------------------------
  // Endpoint control (orchestration — owns the in-flight start promise)
  // ---------------------------------------------------------------------

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
        await lifecycleModule.startEndpointAsync(deviceStore)
      } finally {
        startingPromise = null
      }
    })()
    await startingPromise
  }

  const restartAfterResumeAsync = async () => {
    if (!running.value) return
    log.info('[P2P-RESUME] Restarting P2P endpoint after app resume')
    try { await lifecycleModule.stopAsync() } catch { /* best-effort */ }
    await startAsync()
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
    refreshStatusAsync: lifecycleModule.refreshStatusAsync,
    loadSharesAsync: sharesModule.loadSharesAsync,
    loadSpaceDevicesAsync: sharesModule.loadSpaceDevicesAsync,
    loadAcceptedInviteEndpointsAsync: sharesModule.loadAcceptedInviteEndpointsAsync,
    loadConfiguredRelayUrlAsync: sharesModule.loadConfiguredRelayUrlAsync,
    saveConfiguredRelayUrlAsync: sharesModule.saveConfiguredRelayUrlAsync,
    startAsync,
    stopAsync: lifecycleModule.stopAsync,
    restartAfterResumeAsync,
    ownerSyncRunning,
    startOwnerSyncAsync,
    stopOwnerSyncAsync,
    forceOwnerSyncAsync,
    addShareAsync: sharesModule.addShareAsync,
    removeShareAsync: sharesModule.removeShareAsync,
    registerDeviceInSpaceAsync: sharesModule.registerDeviceInSpaceAsync,
    unregisterDeviceFromSpaceAsync: sharesModule.unregisterDeviceFromSpaceAsync,
    resolveRequestContext: peersModule.resolveRequestContext,
    remoteListAsync: peersModule.remoteListAsync,
    remoteListAllSharesAsync: peersModule.remoteListAllSharesAsync,
    remoteReadAsync: peersModule.remoteReadAsync,
    remoteWriteAsync: peersModule.remoteWriteAsync,
    remoteCreateDirectoryAsync: peersModule.remoteCreateDirectoryAsync,
    getCapabilityForPeer: peersModule.getCapabilityForPeer,
    checkPeerOnlineAsync: peersModule.checkPeerOnlineAsync,
    localListAsync,
    transfers,
    activeDownloads: computed(() => Array.from(transfers.value.values())),
    /**
     * Sum of EMA-smoothed throughput across all in-flight **downloads**.
     * Uploads share the same `transfers` map but the file-browser chip is
     * labelled as a download indicator — mixing both would make the number
     * lie when a user uploads and downloads simultaneously.
     */
    totalBytesPerSec: computed(() => {
      let total = 0
      for (const t of transfers.value.values()) {
        if (t.direction === 'download') total += t.bytesPerSec
      }
      return total
    }),
    getTransferProgress: transfersModule.getTransferProgress,
    getTransferIdForPath: transfersModule.getTransferIdForPath,
    getTransferPaused: transfersModule.getTransferPaused,
    cancelTransferAsync: transfersModule.cancelTransferAsync,
    pauseTransferAsync: transfersModule.pauseTransferAsync,
    resumeTransferAsync: transfersModule.resumeTransferAsync,
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
