import { invoke } from '@tauri-apps/api/core'
import {
  RUST_EVENTS,
  RustEventGroup,
  type PathType,
  type PeerConnectionChangedEvent,
  type PeerConnectionDiagnostics,
} from '@/lib/rust-events'

export type { PathType } from '@/lib/rust-events'

/**
 * Track the current network path (direct vs relay vs closed) for a set of
 * remote peers.
 *
 * The Rust side spawns a watcher per established connection that emits
 * `peer-storage:connection-changed` whenever iroh switches the selected path
 * or the connection is torn down (see `peer_storage::endpoint::spawn_connection_watcher`).
 * This composable subscribes to that event and keeps a `diagMap` in sync —
 * no polling, no `setInterval`.
 *
 * `refreshAsync` is still exposed as the initial-state probe (run on mount
 * and when the peer list changes) and as a manual refresh hook for the UI
 * (e.g. on hover, when the user wants to force-re-read the current path).
 */
export function usePeerConnectionType(endpointIds: Ref<string[]>) {
  const peerStore = usePeerStorageStore()
  const diagMap = ref<Map<string, PeerConnectionDiagnostics | null>>(new Map())

  /**
   * Read the current state for every peer in `endpointIds` via the on-demand
   * `peer_storage_diagnose_connection` command. After this initial probe the
   * map is kept current by `peer-storage:connection-changed` events.
   */
  const refreshAsync = async () => {
    const ids = endpointIds.value
    if (ids.length === 0 || !peerStore.running) return

    const results = await Promise.allSettled(
      ids.map(async (id) => {
        const diag = await invoke<PeerConnectionDiagnostics | null>(
          'peer_storage_diagnose_connection',
          { nodeId: id },
        )
        return { id, diag }
      }),
    )
    const next = new Map(diagMap.value)
    for (const r of results) {
      if (r.status === 'fulfilled') next.set(r.value.id, r.value.diag)
      // rejected: keep existing cached value, peer not yet contacted — silent.
    }
    diagMap.value = next
  }

  const getPathType = (id: string): PathType | null =>
    diagMap.value.get(id)?.pathType ?? null

  const getRttMs = (id: string): number | null =>
    diagMap.value.get(id)?.rttMs ?? null

  // Re-fetch when peer storage starts up or the peer list grows. Push events
  // handle subsequent path-switches, so no setInterval is needed.
  watch(() => peerStore.running, () => { refreshAsync() })
  watch(endpointIds, () => { refreshAsync() })

  const events = new RustEventGroup()
  onMounted(async () => {
    refreshAsync()
    await events.on<PeerConnectionChangedEvent>(
      RUST_EVENTS.peerConnectionChanged,
      ({ nodeId, diagnostics }) => {
        const next = new Map(diagMap.value)
        next.set(nodeId, diagnostics)
        diagMap.value = next
      },
    )
  })
  onUnmounted(() => { events.dispose() })

  return { getPathType, getRttMs, refreshAsync }
}
