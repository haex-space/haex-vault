import { onScopeDispose } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { useWmTab } from '~/composables/useWmTab'
import {
  acceptHeaderSet,
  applyIcons,
  headerEventMessage,
} from '~/lib/extensions/header'

/**
 * An extension's header for the tab of its frame (spec 090-app-header-bar, FR-018–FR-020,
 * contracts/extension-header.md). The host's `extension-header-set` event carries the validated
 * description; it is accepted only for this frame, and an older one (its number is not higher than
 * the last applied) is dropped, so a later call always wins. Pictures come once per frame session:
 * they are merged into a table kept until the session ends. What the user does to a control is
 * posted to the extension's SDK port.
 *
 * `clear()` empties the tab's header (the document reloads, the frame fails); `reset()` also
 * forgets the pictures and the count (the session ends: "Neu laden", close, unmount).
 */
export function useExtensionHeader(options: {
  /** This frame's id, `null` while none is open. */
  frame: () => string | null
  /** Sends a message to the extension's SDK port. */
  post: (message: unknown) => void
}) {
  const tab = useWmTab()
  let icons: Record<string, string> = {}
  let last = 0

  const unregister = tab.registerHeaderHandler((event) =>
    options.post(headerEventMessage(event, Date.now())),
  )

  let stopped = false
  let unlisten: (() => void) | null = null
  void listen('extension-header-set', (event) => {
    const accepted = acceptHeaderSet(options.frame(), event.payload)
    if (!accepted || accepted.seq <= last) return
    last = accepted.seq
    const applied = applyIcons(icons, accepted.header)
    icons = applied.table
    tab.setHeader(applied.header)
  }).then((off) => {
    if (stopped) off()
    else unlisten = off
  })

  function clear(): void {
    tab.setHeader(null)
  }

  function reset(): void {
    icons = {}
    last = 0
    clear()
  }

  onScopeDispose(() => {
    stopped = true
    unlisten?.()
    unregister()
    reset()
  })

  return { clear, reset }
}
