import { computed, ref, watch, type Ref } from 'vue'
import type { Thread } from '~/composables/useChat'
import type { TabRouter } from '~/composables/useTabRouter'

const THREAD_PREFIX = '/thread/'
const CONTACT_PREFIX = '/contact/'

/** The chat tab's location for a conversation (`/` = new conversation, the start location). */
export function chatPath(threadId: string | null): string {
  return threadId === null
    ? '/'
    : `${THREAD_PREFIX}${encodeURIComponent(threadId)}`
}

/** The chat tab's location for a contact or group conversation (spec 072). */
export function contactChatPath(threadId: string): string {
  return `${CONTACT_PREFIX}${encodeURIComponent(threadId)}`
}

function idAfter(prefix: string, path: string): string | null {
  if (!path.startsWith(prefix)) return null
  try {
    return decodeURIComponent(path.slice(prefix.length))
  } catch {
    return null
  }
}

const threadIdOf = (path: string) => idAfter(THREAD_PREFIX, path)
const peerIdOf = (path: string) => idAfter(CONTACT_PREFIX, path)

/**
 * Chat ↔ tab history (spec 020-tab-navigation, T025/T055, research R10). The
 * chat has two locations: `/` (new conversation, the start location — spec 004
 * still begins every chat entry there) and `/thread/<id>`. Kept in sync both
 * ways:
 *
 * - location → chat: reaching `/thread/<id>` runs the existing `selectThread`,
 *   reaching `/` runs `newChat`, so running replies and pending approvals keep
 *   following specs 003/006 (FR-027);
 * - chat → location: when the active conversation changes without a
 *   navigation (the first message creates the thread, the active thread is
 *   deleted, `newChat` refuses while a reply runs), the current entry is
 *   replaced, so back never lands on an empty new conversation.
 *
 * Spec 072 adds a third location, `/contact/<id>`, for a contact or group conversation. It only
 * sets `activePeerId`; the agent conversation underneath keeps its state, so a running reply goes
 * on while the user reads a contact's message, and nothing is aligned while it is open.
 *
 * Takes the router as a parameter so the chat replay harness can pass its own.
 */
export function useChatNavigation(deps: {
  router: TabRouter
  activeThreadId: Ref<string | null>
  threads: Ref<Thread[]>
  /** Header/tab title of a conversation that has no title yet. */
  newChatLabel: () => string
  /** `useWmTab().setTitle`: the conversation title becomes the tab title (spec 020 US5). */
  setTitle: (title: string | null) => void
  selectThread: (id: string) => Promise<void> | void
  newChat: () => Promise<void> | void
  /** Spec 072: the open contact or group conversation; absent in harnesses without one. */
  activePeerId?: Ref<string | null>
  /** Spec 072: tab title of a contact or group conversation. */
  peerTitle?: (id: string) => string | null
}) {
  const { router, activeThreadId, threads } = deps
  const activePeerId = deps.activePeerId ?? ref<string | null>(null)

  const chatTitle = computed(
    () =>
      (activeThreadId.value &&
        threads.value.find((thread) => thread.id === activeThreadId.value)
          ?.title) ||
      deps.newChatLabel(),
  )

  function openConversation(threadId: string) {
    router.push(chatPath(threadId))
  }

  function openPeerConversation(threadId: string) {
    router.push(contactChatPath(threadId))
  }

  function startNewConversation() {
    router.push('/')
  }

  function alignLocation() {
    if (activePeerId.value !== null) return
    const expected = chatPath(activeThreadId.value)
    if (router.route.path !== expected) router.replace(expected)
  }

  let threadsLoaded = false
  async function applyLocation(path: string) {
    const peerId = peerIdOf(path)
    if (peerId !== null) {
      activePeerId.value = peerId
      return
    }
    activePeerId.value = null
    const threadId = threadIdOf(path)
    if (threadId === activeThreadId.value) return
    if (threadId !== null && !threads.value.some((t) => t.id === threadId)) {
      // The list is not loaded yet: decide once `syncFromLocation` runs.
      if (!threadsLoaded) return
      // FR-027: an entry of a deleted conversation is skipped in travel direction.
      if (!router.skipCurrent()) router.replace('/')
      return
    }
    if (threadId !== null) await deps.selectThread(threadId)
    else await deps.newChat()
    alignLocation()
  }
  watch(() => router.route.path, applyLocation)

  /** Call once the thread list has loaded: applies the location the tab started at. */
  function syncFromLocation() {
    threadsLoaded = true
    return applyLocation(router.route.path)
  }

  watch(activeThreadId, alignLocation)

  // The tab shows the open conversation's title; a new conversation keeps the app/route title.
  // The window manager resets this override on every navigation, so it is set again for the new view.
  const peerTitle = computed(() =>
    activePeerId.value !== null
      ? (deps.peerTitle?.(activePeerId.value) ?? null)
      : null,
  )
  watch(
    [
      activeThreadId,
      chatTitle,
      activePeerId,
      peerTitle,
      () => router.route.path,
    ],
    () =>
      deps.setTitle(
        activePeerId.value !== null
          ? peerTitle.value
          : activeThreadId.value
            ? chatTitle.value
            : null,
      ),
    { immediate: true },
  )

  return {
    chatTitle,
    activePeerId,
    openConversation,
    openPeerConversation,
    startNewConversation,
    syncFromLocation,
  }
}
