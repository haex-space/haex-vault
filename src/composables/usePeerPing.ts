import {
  RUST_EVENTS,
  RustEventGroup,
  type PeerConnectionChangedEvent,
} from '@/lib/rust-events'

export type PeerPingStatus = 'checking' | 'online' | 'offline'

/**
 * Track online/offline state for a set of remote peers.
 *
 * Initial state is determined by an active probe (`checkPeerOnlineAsync`,
 * which issues a real `peer_storage_remote_list` request — the only way to
 * know whether we can talk to a peer we have not opened a connection to yet).
 * After that the state is kept current by `peer-storage:connection-changed`
 * events emitted from Rust whenever iroh switches the selected path or the
 * connection drops — no `setInterval`.
 *
 * Limitation: a peer that goes from offline → online without the user
 * triggering a request against it will continue to show offline until the
 * next active probe (e.g. another consumer of this composable mounting, or a
 * file-browser request). That is the trade-off for cutting the 30s poll:
 * offline-to-online transitions need either a UI action or an explicit
 * `refreshAsync()` call. The `connection-changed` event fires whenever a
 * connection IS established or torn down, so a successful new request
 * propagates online status to all subscribers via the watcher in
 * `peer_storage::endpoint::spawn_connection_watcher`.
 */
export function usePeerPing(endpointIds: Ref<string[]>) {
  const peerStore = usePeerStorageStore()
  const status = ref<Map<string, PeerPingStatus>>(new Map())

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

    await Promise.all(
      ids.map(async (id) => {
        const alive = await peerStore.checkPeerOnlineAsync(id)
        status.value.set(id, alive ? 'online' : 'offline')
        status.value = new Map(status.value)
      }),
    )
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
  onUnmounted(() => { events.dispose() })

  return { getStatus, refreshAsync: pingAllAsync }
}
