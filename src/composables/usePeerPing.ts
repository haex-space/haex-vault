import { useDocumentVisibility, useIntervalFn } from '@vueuse/core'
import {
  RUST_EVENTS,
  RustEventGroup,
  type PeerConnectionChangedEvent,
} from '@/lib/rust-events'

export type PeerPingStatus = 'checking' | 'online' | 'offline'

/**
 * How often the sparse "is anyone offline come back?" probe runs. Online
 * peers are not probed — `peer-storage:connection-changed` events already
 * cover their lifecycle. Gated by document visibility so the wakeups stop
 * when the tab is hidden / the system is locked.
 */
const OFFLINE_PROBE_INTERVAL_MS = 60_000

/**
 * Track online/offline state for a set of remote peers.
 *
 * Three signal sources, layered:
 *
 *  - **Initial probe on mount.** `pingAllAsync` calls `checkPeerOnlineAsync`
 *    (which issues `peer_storage_remote_list`) so the first dot render has a
 *    real value instead of `checking`.
 *  - **Push events.** `peer-storage:connection-changed` keeps the map current
 *    while a connection exists: path switches and drops show up immediately.
 *    Outbound _and_ inbound connections both spawn watchers on the Rust side.
 *  - **Sparse offline-only heartbeat.** A 60s interval re-probes _only_ peers
 *    currently marked `offline`. This is the offline→online detector for
 *    peers we never actively request data from (e.g. a remote peer sitting in
 *    the file-browser sidebar). The probe is visibility-gated — when the
 *    document is hidden the interval is paused, so the wakeups stop while
 *    no one is looking.
 *
 * Plus `refreshOne(id)` for explicit per-peer refresh (hover, manual button).
 */
export function usePeerPing(endpointIds: Ref<string[]>) {
  const peerStore = usePeerStorageStore()
  const status = ref<Map<string, PeerPingStatus>>(new Map())

  /** Probe a single peer. Used by `refreshOne` (hover) and the heartbeat. */
  const probeOneAsync = async (id: string): Promise<void> => {
    if (!peerStore.running) {
      const next = new Map(status.value)
      next.set(id, 'offline')
      status.value = next
      return
    }
    const alive = await peerStore.checkPeerOnlineAsync(id)
    const next = new Map(status.value)
    next.set(id, alive ? 'online' : 'offline')
    status.value = next
  }

  const pingAllAsync = async () => {
    const ids = endpointIds.value
    if (ids.length === 0) return

    if (!peerStore.running) {
      for (const id of ids) {
        status.value.set(id, 'offline')
      }
      status.value = new Map(status.value)
      return
    }

    await Promise.all(ids.map(probeOneAsync))
  }

  /** Heartbeat: only re-probe peers currently offline. */
  const pingOfflineAsync = async () => {
    if (!peerStore.running) return
    const offlineIds = endpointIds.value.filter(
      (id) => status.value.get(id) === 'offline',
    )
    if (offlineIds.length === 0) return
    await Promise.all(offlineIds.map(probeOneAsync))
  }

  /** Per-peer refresh — wired to the StatusDot's hover handler. */
  const refreshOne = async (id: string): Promise<void> => {
    await probeOneAsync(id)
  }

  const getStatus = (id: string): PeerPingStatus => status.value.get(id) ?? 'checking'

  // Re-ping immediately when the peer store starts or stops so the dot
  // reflects the new state.
  watch(() => peerStore.running, () => { pingAllAsync() })

  // Mark newly discovered peers as 'checking' and re-ping when the list
  // changes. No immediate:true — avoids firing during setup() before all
  // const declarations in the parent scope are initialized (TDZ).
  watch(endpointIds, (ids) => {
    for (const id of ids) {
      if (!status.value.has(id)) {
        status.value.set(id, 'checking')
        status.value = new Map(status.value)
      }
    }
    pingAllAsync()
  })

  // Listen for connection-changed events so a relay→direct switch or a
  // sudden drop reaches the UI without us re-pinging the world.
  const events = new RustEventGroup()

  // Sparse offline-only heartbeat, gated by document visibility. `immediate:
  // false` so we don't fire before onMounted's initial probe; the visibility
  // watcher below resumes it once the page is visible.
  const visibility = useDocumentVisibility()
  const heartbeat = useIntervalFn(pingOfflineAsync, OFFLINE_PROBE_INTERVAL_MS, {
    immediate: false,
  })
  watch(
    visibility,
    (v) => {
      if (v === 'visible') heartbeat.resume()
      else heartbeat.pause()
    },
    { immediate: true },
  )

  onMounted(async () => {
    for (const id of endpointIds.value) {
      status.value.set(id, 'checking')
    }
    status.value = new Map(status.value)
    pingAllAsync()

    await events.on<PeerConnectionChangedEvent>(
      RUST_EVENTS.peerConnectionChanged,
      ({ nodeId, diagnostics }) => {
        const next = new Map(status.value)
        next.set(nodeId, diagnostics.pathType === 'closed' ? 'offline' : 'online')
        status.value = next
      },
    )
  })
  onUnmounted(() => {
    events.dispose()
    heartbeat.pause()
  })

  return { getStatus, refreshAsync: pingAllAsync, refreshOne }
}
