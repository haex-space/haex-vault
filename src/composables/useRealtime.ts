import { createDidAuthToken } from '../utils/auth/didAuth'

export interface RealtimeEvent {
  type: 'sync' | 'membership' | 'mls' | 'invite'
  spaceId: string
  inviteId?: string
}

export function useRealtime() {
  const connected = ref(false)

  let ws: WebSocket | null = null
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null
  let reconnectAttempts = 0
  let intentionalDisconnect = false
  let lastConnectArgs: { originUrl: string; privateKeyBase64: string; did: string } | null = null

  const handlers = new Map<string, Set<(event: RealtimeEvent) => void>>()

  async function connect(originUrl: string, privateKeyBase64: string, did: string) {
    lastConnectArgs = { originUrl, privateKeyBase64, did }
    intentionalDisconnect = false

    const token = await createDidAuthToken(privateKeyBase64, did)
    const wsUrl = `${originUrl.replace(/^http/, 'ws')}/ws?token=${encodeURIComponent(token)}`

    ws = new WebSocket(wsUrl)

    ws.onopen = () => {
      connected.value = true
      reconnectAttempts = 0
    }

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data) as RealtimeEvent
        const typeHandlers = handlers.get(data.type)
        if (typeHandlers) {
          for (const handler of typeHandlers) {
            handler(data)
          }
        }
      } catch {
        // Ignore malformed messages
      }
    }

    ws.onclose = () => {
      connected.value = false
      ws = null

      if (!intentionalDisconnect && lastConnectArgs) {
        scheduleReconnect()
      }
    }

    ws.onerror = () => {
      // onerror is always followed by onclose, reconnect handled there
    }
  }

  function disconnect() {
    intentionalDisconnect = true
    clearReconnectTimer()

    if (ws) {
      ws.close()
      ws = null
    }

    connected.value = false
  }

  function on(type: string, handler: (event: RealtimeEvent) => void): () => void {
    if (!handlers.has(type)) {
      handlers.set(type, new Set())
    }
    handlers.get(type)!.add(handler)

    return () => {
      const typeHandlers = handlers.get(type)
      if (typeHandlers) {
        typeHandlers.delete(handler)
        if (typeHandlers.size === 0) {
          handlers.delete(type)
        }
      }
    }
  }

  function scheduleReconnect() {
    clearReconnectTimer()
    const delay = Math.min(1000 * 2 ** reconnectAttempts, 30000)
    reconnectAttempts++

    reconnectTimer = setTimeout(() => {
      if (lastConnectArgs && !intentionalDisconnect) {
        connect(lastConnectArgs.originUrl, lastConnectArgs.privateKeyBase64, lastConnectArgs.did)
      }
    }, delay)
  }

  function clearReconnectTimer() {
    if (reconnectTimer !== null) {
      clearTimeout(reconnectTimer)
      reconnectTimer = null
    }
  }

  return { connected, connect, disconnect, on }
}
