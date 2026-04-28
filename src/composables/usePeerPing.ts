export type PeerPingStatus = 'checking' | 'online' | 'offline'

const POLL_INTERVAL_MS = 30_000

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

  // Mark newly discovered peers as 'checking' and re-ping when the list changes.
  // No immediate:true — avoids firing during setup() before all const declarations
  // in the parent scope are initialized (TDZ).
  watch(endpointIds, (ids) => {
    for (const id of ids) {
      if (!status.value.has(id)) {
        status.value.set(id, 'checking')
        status.value = new Map(status.value)
      }
    }
    pingAllAsync()
  })

  let timer: ReturnType<typeof setInterval> | undefined
  onMounted(() => {
    for (const id of endpointIds.value) {
      status.value.set(id, 'checking')
    }
    status.value = new Map(status.value)
    pingAllAsync()
    timer = setInterval(pingAllAsync, POLL_INTERVAL_MS)
  })
  onUnmounted(() => { clearInterval(timer) })

  return { getStatus, refreshAsync: pingAllAsync }
}
