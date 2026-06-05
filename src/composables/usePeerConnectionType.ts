import { invoke } from '@tauri-apps/api/core'

export type PathType = 'direct' | 'relay' | 'unknown' | 'closed'

interface ConnectionDiagnostics {
  pathType: PathType
  remoteAddr: string | null
  rttMs: number | null
}

const POLL_INTERVAL_MS = 10_000

export function usePeerConnectionType(endpointIds: Ref<string[]>) {
  const peerStore = usePeerStorageStore()
  const diagMap = ref<Map<string, ConnectionDiagnostics | null>>(new Map())

  const refreshAsync = async () => {
    const ids = endpointIds.value
    if (ids.length === 0 || !peerStore.running) return

    await Promise.all(
      ids.map(async (id) => {
        try {
          const diag = await invoke<ConnectionDiagnostics | null>(
            'peer_storage_diagnose_connection',
            { nodeId: id },
          )
          diagMap.value.set(id, diag)
          diagMap.value = new Map(diagMap.value)
        } catch {
          // Endpoint not running or peer not yet contacted — silent.
        }
      }),
    )
  }

  const getPathType = (id: string): PathType | null =>
    diagMap.value.get(id)?.pathType ?? null

  const getRttMs = (id: string): number | null =>
    diagMap.value.get(id)?.rttMs ?? null

  watch(() => peerStore.running, () => { refreshAsync() })
  watch(endpointIds, () => { refreshAsync() })

  let timer: ReturnType<typeof setInterval> | undefined
  onMounted(() => {
    refreshAsync()
    timer = setInterval(refreshAsync, POLL_INTERVAL_MS)
  })
  onUnmounted(() => { clearInterval(timer) })

  return { getPathType, getRttMs, refreshAsync }
}
