import { nextTick, onBeforeUnmount, ref, watch, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { ExtensionStatusChanged } from '@bindings/ExtensionStatusChanged'
import type { FrameOpened } from '@bindings/FrameOpened'
import {
  DEV_CONSOLE_LINES,
  readConsoleForward,
  type DevConsoleLine,
} from '~/lib/extensions/devConsole'
import { ALL_ACTIONS } from '~/lib/actions/catalog'
import {
  FrameEventQueue,
  encodeBytes,
  eventMessage,
  readRequest,
  type FrameEvent,
} from '~/lib/extensions/bridge'
import { readyFrames } from '~/lib/extensions/agentFrames'
import { mcpPortMessage, readMcpMessage } from '~/lib/extensions/mcpRelay'
import {
  initMessage,
  navigateMessage,
  readShimMessage,
  sameLocation,
} from '~/lib/extensions/shim-protocol'
import { detectPlatform, embeddedShortcuts } from '~/lib/wm/keybindings'
import { useFrameStorageDialog } from '~/composables/useStorageRequests'
import { formatLocation, type TabLocation } from '~/lib/wm/navigation'
import { useErrorString } from '~/composables/useErrorString'
import { useTabRouter } from '~/composables/useTabRouter'
import { useWmTab } from '~/composables/useWmTab'

const PORT_INIT = 'haexspace:port:init'
const PORT_READY = 'haexspace:port:ready'
const INIT_INTERVAL_MS = 200
const INIT_TIMEOUT_MS = 10_000

export type FrameState = 'loading' | 'ready' | 'error'

/** A confirmation the extension asked for (`extension_dialog_confirm`), shown over this tab. */
export type FrameDialog = {
  requestId: string
  message: string
  title: string | null
  confirmLabel: string | null
  cancelLabel: string | null
  destructive: boolean
}

/**
 * One extension frame in a tab (spec 017, T046, contracts/bridge.md, research R13, R17): opens a
 * frame session in Rust, hands the SDK its port for **every** new document (a reload gets a new
 * channel; the shim's `hello` tells a new document from a `load` that WebKitGTK fires for a
 * fragment navigation inside the frame), relays requests to `extension_bridge_call` unchanged, holds events until the SDK is ready, and
 * maps the frame shim's messages onto this tab only.
 *
 * ponytail: moving the tab to another window or switching the workspace remounts the frame, so
 * the extension reloads (R17); keeping frames alive across moves needs a frame pool outside the
 * tab panels.
 */
export function useExtensionFrame(
  iframe: Ref<HTMLIFrameElement | null>,
  extensionId: string,
) {
  const tab = useWmTab()
  const router = useTabRouter()
  const wm = useWindowManagerStore()
  const { errString } = useErrorString()
  const { t } = useI18n()

  const state = ref<FrameState>('loading')
  const error = ref<string | null>(null)
  const src = ref<string | null>(null)
  const dialog = ref<FrameDialog | null>(null)
  /** A storage request of this frame (spec 038), over the tab like `dialog`. */
  const { storageRequest, storageCredentialsPending, answerStorage } =
    useFrameStorageDialog(
      () => frame,
      () => void nextTick(() => iframe.value?.focus()),
    )
  /** A development version (spec 017, US12): its console output is shown. */
  const dev = ref(false)
  const consoleLines = ref<DevConsoleLine[]>([])

  let frame: string | null = null
  let sdkPort: MessagePort | null = null
  let shimPort: MessagePort | null = null
  let attempts: MessageChannel[] = []
  let initTimer: ReturnType<typeof setInterval> | null = null
  let initDeadline: ReturnType<typeof setTimeout> | null = null
  let devHandshakeStarted = false
  let unregisterGuard: (() => void) | null = null
  let lastShimLocation: TabLocation | null = null
  const unlisten: UnlistenFn[] = []
  // Counts closes: an open that resolves after a close (unmount, "Neu laden") is closed at once.
  let generation = 0
  let unmounted = false
  const shortcuts = embeddedShortcuts(
    ALL_ACTIONS,
    detectPlatform(
      navigator as Navigator & { userAgentData?: { platform?: string } },
    ),
  )

  // Replaced by the frame's own queue once the session is open.
  let events = new FrameEventQueue('', () => {})

  function currentHash(): string {
    return (
      '#' +
      formatLocation({ path: router.route.path, query: router.route.query })
    )
  }

  function stopInit(): void {
    if (initTimer) clearInterval(initTimer)
    if (initDeadline) clearTimeout(initDeadline)
    initTimer = null
    initDeadline = null
  }

  function closeSdkPorts(): void {
    stopInit()
    for (const attempt of attempts) attempt.port1.close()
    attempts = []
    sdkPort?.close()
    sdkPort = null
  }

  function closePorts(): void {
    devHandshakeStarted = false
    closeSdkPorts()
    shimPort?.close()
    shimPort = null
  }

  /** The frame does not answer in time: shown as an error with "Neu laden". */
  function armDeadline(): void {
    if (initDeadline) clearTimeout(initDeadline)
    initDeadline = setTimeout(() => {
      stopInit()
      state.value = 'error'
      error.value = t('extensions.frame.timeout')
    }, INIT_TIMEOUT_MS)
  }

  async function relay(port: MessagePort, data: unknown): Promise<void> {
    // The extension's answers to the agent's tool calls (spec 018), apart from the bridge.
    const mcp = readMcpMessage(data)
    if (mcp) {
      if (frame)
        void invoke('extension_mcp_send', { frame, message: mcp }).catch(
          () => {},
        )
      return
    }
    const request = readRequest(data)
    if (!request || !frame) return
    const response = await invoke<unknown>('extension_bridge_call', {
      frame,
      id: request.id,
      method: request.method,
      params: encodeBytes(request.params),
    }).catch((e: unknown) => ({
      id: request.id,
      error: { code: 1000, message: errString(e) },
    }))
    port.postMessage(response)
  }

  /** One handshake attempt: a fresh channel, since a port can be transferred only once. */
  function offerPort(): void {
    const target = iframe.value?.contentWindow
    if (!target) return
    const channel = new MessageChannel()
    attempts.push(channel)
    channel.port1.onmessage = (event: MessageEvent) => {
      if (sdkPort === null && event.data?.type === PORT_READY) {
        stopInit()
        sdkPort = channel.port1
        for (const other of attempts) if (other !== channel) other.port1.close()
        attempts = []
        state.value = 'ready'
        events.ready()
        if (frame) readyFrames.ready(tab.tabId, frame)
        return
      }
      if (sdkPort === channel.port1) void relay(channel.port1, event.data)
    }
    target.postMessage({ type: PORT_INIT }, '*', [channel.port2])
  }

  function applyShim(data: unknown): void {
    const effect = readShimMessage(data, shortcuts)
    if (!effect) return
    switch (effect.kind) {
      case 'hello':
        // WebKitGTK can deliver the development page before its initialization script has
        // installed the message listener. The load-time fallback below may already have offered
        // the SDK port in that case; do not reset that live handshake when the late shim hello
        // arrives.
        if (effect.fresh && !devHandshakeStarted) {
          if (dev.value) devHandshakeStarted = true
          startHandshake()
        }
        return
      case 'navigate': {
        lastShimLocation = effect.location
        const current = { path: router.route.path, query: router.route.query }
        if (sameLocation(current, effect.location)) return
        if (effect.replace) router.replace(effect.location)
        else router.push(effect.location)
        return
      }
      case 'title':
        tab.setTitle(effect.title || null)
        return
      case 'closeGuard':
        unregisterGuard?.()
        unregisterGuard = effect.active
          ? tab.registerCloseGuard(() => ({
              reasonKey: 'extensions.frame.closeGuard',
              confirmAsync: async () => {},
            }))
          : null
        return
      case 'close':
        tab.closeSelf()
        return
      case 'focus':
        // A click into the frame never reaches the window's own pointer handler. Only while
        // holzi's window is active, so a frame cannot raise itself in the background.
        if (document.hasFocus()) wm.focusWindow(tab.windowId)
        return
      case 'shortcut':
        // Only a key the user pressed while the frame has the focus (contracts/bridge.md).
        if (document.hasFocus() && document.activeElement === iframe.value)
          void wm.runAction(effect.actionId, {}, { kind: 'user' })
    }
  }

  function startShim(): void {
    const target = iframe.value?.contentWindow
    if (!target) return
    shimPort?.close()
    const channel = new MessageChannel()
    shimPort = channel.port1
    shimPort.onmessage = (event: MessageEvent) => applyShim(event.data)
    target.postMessage(initMessage(shortcuts), '*', [channel.port2])
  }

  /** A new document in the frame: a fresh SDK channel. */
  function startHandshake(): void {
    readyFrames.gone(tab.tabId)
    closeSdkPorts()
    events.reset()
    // The old page's unacknowledged shell output is lost; its shells stop waiting for it.
    if (frame)
      void invoke('extension_frame_reloaded', { frame }).catch(() => {})
    state.value = 'loading'
    offerPort()
    initTimer = setInterval(offerPort, INIT_INTERVAL_MS)
    armDeadline()
  }

  /** Every `load` of the frame; the shim's `hello` then says whether its document is new. A
   * development page starts its SDK handshake directly after load because its init script may
   * run before the page's message listener and the one-shot shim hello can then be lost. */
  function onLoad(): void {
    if (!frame) return
    startShim()
    if (state.value !== 'ready') {
      armDeadline()
      if (dev.value && !devHandshakeStarted) {
        devHandshakeStarted = true
        startHandshake()
      }
    }
  }

  async function openAsync(): Promise<void> {
    if (unmounted) return
    const opening = generation
    state.value = 'loading'
    error.value = null
    try {
      const opened = await invoke<FrameOpened>('extension_frame_open', {
        extensionId,
        tabId: tab.tabId,
      })
      if (opening !== generation) {
        await invoke('extension_frame_close', { frame: opened.frame }).catch(
          () => {},
        )
        return
      }
      frame = opened.frame
      dev.value = opened.dev
      events = new FrameEventQueue(opened.frame, (event: FrameEvent) =>
        sdkPort?.postMessage(eventMessage(event)),
      )
      src.value = opened.url + currentHash()
      // A WebKitGTK load event can be lost while the development page is already rendered. Run
      // the same direct handshake after Vue has mounted the frame; the interval in
      // startHandshake() keeps retrying until the page's listener exists.
      if (opened.dev) void nextTick(() => onLoad())
    } catch (e) {
      if (opening !== generation) return
      state.value = 'error'
      error.value = errString(e)
    }
  }

  /** Answers the open dialog; closing it counts as "cancel". The keyboard goes back to the frame,
   * as after `window.confirm()`, once the frame is no longer inert. */
  function answerDialog(confirmed: boolean): void {
    const open = dialog.value
    dialog.value = null
    if (!open) return
    void invoke('extension_dialog_resolve', {
      requestId: open.requestId,
      confirmed,
    }).catch(() => {})
    void nextTick(() => requestAnimationFrame(() => iframe.value?.focus()))
  }

  async function closeAsync(): Promise<void> {
    generation++
    readyFrames.gone(tab.tabId)
    dialog.value = null
    closePorts()
    unregisterGuard?.()
    unregisterGuard = null
    const closing = frame
    frame = null
    src.value = null
    if (closing)
      await invoke('extension_frame_close', { frame: closing }).catch(() => {})
  }

  /** "Neu laden": a new frame session and a fresh document. */
  async function reloadAsync(): Promise<void> {
    await closeAsync()
    await openAsync()
  }

  // holzi Back/Forward: show the tab's location in the frame, unless the frame reported it.
  watch(
    () => ({ path: router.route.path, query: router.route.query }),
    (location) => {
      if (lastShimLocation && sameLocation(location, lastShimLocation)) return
      lastShimLocation = location
      shimPort?.postMessage(navigateMessage(location))
    },
  )

  // Console output of the extension counts only from this frame's own window.
  function onWindowMessage(event: MessageEvent): void {
    if (!iframe.value || event.source !== iframe.value.contentWindow) return
    if (dev.value) {
      const line = readConsoleForward(event.data)
      if (line)
        consoleLines.value = [...consoleLines.value, line].slice(
          -DEV_CONSOLE_LINES,
        )
      return
    }
    const data = event.data as { type?: unknown } | null
    if (
      data &&
      typeof data.type === 'string' &&
      data.type.startsWith('haexspace:')
    )
      console.debug('[extension]', extensionId, data)
  }
  window.addEventListener('message', onWindowMessage)

  void Promise.all([
    listen<FrameEvent>('extension-frame-event', (event) =>
      events.push(event.payload),
    ),
    listen<{ frame: string; message: unknown }>(
      'extension-mcp-message',
      (event) => {
        if (event.payload.frame === frame)
          sdkPort?.postMessage(mcpPortMessage(event.payload.message))
      },
    ),
    listen<{ frame: string; active: boolean }>(
      'extension-tab-attention',
      (event) => {
        if (event.payload.frame !== frame) return
        if (event.payload.active) tab.requestAttention()
        else tab.clearAttention()
      },
    ),
    // A new effective bundle (an update from any own device): this tab loads it (FR-038).
    listen<ExtensionStatusChanged>('extension-status-changed', (event) => {
      if (event.payload.extensionId === extensionId && event.payload.reload)
        void reloadAsync()
    }),
    listen<FrameDialog & { frame: string }>(
      'extension-dialog-request',
      (event) => {
        const request = event.payload
        if (request.frame !== frame) return
        dialog.value = {
          requestId: request.requestId,
          message: request.message,
          title: request.title,
          confirmLabel: request.confirmLabel,
          cancelLabel: request.cancelLabel,
          destructive: request.destructive,
        }
      },
    ),
  ]).then((offs) => {
    if (unmounted) for (const off of offs) off()
    else unlisten.push(...offs)
  })

  onBeforeUnmount(() => {
    unmounted = true
    window.removeEventListener('message', onWindowMessage)
    for (const off of unlisten) off()
    void closeAsync()
  })

  void openAsync()

  return {
    state,
    error,
    src,
    dialog,
    dev,
    consoleLines,
    answerDialog,
    storageRequest,
    storageCredentialsPending,
    answerStorage,
    onLoad,
    reloadAsync,
  }
}
