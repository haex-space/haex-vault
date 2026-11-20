import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { computed, onScopeDispose, ref, watch, type Ref } from 'vue'
import type { ChatPeerMessage } from '@bindings/ChatPeerMessage'
import { useConversationCommands } from '~/composables/useConversations'
import { useWmTab } from '~/composables/useWmTab'
import { useWindowManagerStore } from '~/stores/windowManager'

/**
 * The most characters a message or note may have (spec 072 FR-012, `envelope::MAX_TEXT_CHARS`). A
 * text that does not fit one wrap is sent as a file, so this is the only limit the window knows.
 */
export const MAX_TEXT_CHARS = 1_000_000
/** The character count shows from here on. */
export const COUNTER_FROM = MAX_TEXT_CHARS - 10_000

/**
 * The messages of one contact conversation (spec 072 US2, FR-021–FR-024): loaded by `sortAt`,
 * refreshed whenever chat rows change (here, on another own device or by a delivery), and marked
 * read while the conversation is the active tab of a visible, focused window. While it is, holzi
 * shows no notification for it (FR-023): the view tells Rust under an id of its own.
 */
export function usePeerConversation(
  threadId: Ref<string | null>,
  errString: (error: unknown) => string,
) {
  const commands = useConversationCommands()
  const wmTab = useWmTab()
  const wm = useWindowManagerStore()

  const messages = ref<ChatPeerMessage[]>([])
  const error = ref<string | null>(null)
  const loading = ref(false)

  // action-exempt: which conversation a view shows is view state, not an action.
  const viewId = crypto.randomUUID()
  const focused = ref(typeof document === 'undefined' || document.hasFocus())
  const visible = ref(
    typeof document === 'undefined' || document.visibilityState === 'visible',
  )
  const onFocus = () => (focused.value = true)
  const onBlur = () => (focused.value = false)
  const onVisibility = () =>
    (visible.value = document.visibilityState === 'visible')
  if (typeof window !== 'undefined') {
    window.addEventListener('focus', onFocus)
    window.addEventListener('blur', onBlur)
    document.addEventListener('visibilitychange', onVisibility)
  }

  /** The tab is the active one of a window that is shown in the current workspace. */
  const tabShown = computed(() => {
    const own = wm.windows.find((w) => w.id === wmTab.windowId)
    // Outside the window manager (harnesses) the view counts as shown.
    if (!own) return true
    return (
      own.activeTabId === wmTab.tabId &&
      !own.minimized &&
      own.workspaceId === wm.activeWorkspaceId
    )
  })
  const seen = computed(
    () =>
      threadId.value !== null &&
      tabShown.value &&
      focused.value &&
      visible.value,
  )

  watch(
    () => (seen.value ? threadId.value : null),
    (viewing) => {
      commands.viewingAsync(viewId, viewing).catch(() => {})
    },
    { immediate: true },
  )

  /** The newest message of the other side that is not marked read yet. */
  let readUpTo = 0
  async function markRead() {
    const id = threadId.value
    if (!id || !seen.value) return
    const newest = messages.value.reduce(
      (at, m) => (m.own ? at : Math.max(at, m.sortAt)),
      0,
    )
    if (newest <= readUpTo) return
    readUpTo = newest
    try {
      await commands.readAsync(id)
    } catch (e: unknown) {
      readUpTo = 0
      error.value = errString(e)
    }
  }

  async function refresh() {
    const id = threadId.value
    if (!id) {
      messages.value = []
      return
    }
    loading.value = true
    try {
      const list = await commands.messagesAsync(id)
      if (threadId.value !== id) return
      messages.value = list
      error.value = null
      await markRead()
    } catch (e: unknown) {
      error.value = errString(e)
    } finally {
      loading.value = false
    }
  }

  watch(
    threadId,
    () => {
      readUpTo = 0
      messages.value = []
      void refresh()
    },
    { immediate: true },
  )
  watch(seen, (now) => {
    if (now) void markRead()
  })
  onVaultTablesChanged(
    ['chat_peer_messages', 'chat_deliveries', 'chat_threads'],
    refresh,
  )

  // The copy of a file lives on this device only, so a finished transfer is no vault change: the
  // text of a long message appears when its file is stored (FR-012a).
  let unlistenTransfers: UnlistenFn | null = null
  let disposed = false
  if (typeof window !== 'undefined') {
    listen<{
      attachmentId: string
      bytesDone: number
      state: string
    }>('chat-transfer', ({ payload }) => {
      if (payload.state === 'downloading') {
        for (const message of messages.value) {
          const attachment = message.attachments.find(
            (item) => item.id === payload.attachmentId,
          )
          if (attachment) {
            attachment.state = payload.state
            attachment.bytesDone = payload.bytesDone
            break
          }
        }
      } else {
        void refresh()
      }
    })
      .then((unlisten) => {
        if (disposed) unlisten()
        else unlistenTransfers = unlisten
      })
      .catch(() => {})
  }

  /** Fetches a file now (retry, or above the automatic limit); its state follows by event. */
  async function download(attachmentId: string) {
    try {
      await commands.downloadAsync(attachmentId)
    } catch (e: unknown) {
      error.value = errString(e)
    }
    await refresh()
  }

  onScopeDispose(() => {
    disposed = true
    unlistenTransfers?.()
    commands.viewingAsync(viewId, null).catch(() => {})
    if (typeof window !== 'undefined') {
      window.removeEventListener('focus', onFocus)
      window.removeEventListener('blur', onBlur)
      document.removeEventListener('visibilitychange', onVisibility)
    }
  })

  return { messages, error, loading, refresh, download }
}
