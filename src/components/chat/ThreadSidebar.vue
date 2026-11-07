<script setup lang="ts">
/**
 * The chat's list, like a messenger's (spec 072 FR-001, FR-002, FR-002b, revision 2026-11-01): one
 * entry per contact and group, exactly one "Agent" entry and one "Ich" entry for the notes, newest
 * activity first. The agent's sessions are not listed here; its header offers them ("Verlauf").
 * Requests (FR-002, FR-040) are not in the list either: an entry "Anfragen" with their count leads
 * to them, as a list of its own in the sidebar.
 * Where the sidebar sits and how wide it is, the frame (`SidebarLayout.vue`) decides; it fills what
 * it is given. Spec 076: it has its own header (toggle, "Chats", new conversation) and, collapsed
 * (`collapsed`), shows only one avatar per entry, in the same order. Expanded, a search field
 * (spec 076 US2) narrows the list by name at once and lists the messages that contain the text.
 */
import { Search } from '@lucide/vue'
import { toast } from 'vue-sonner'
import type { ChatSearchHit } from '@bindings/ChatSearchHit'
import type { Thread } from '~/composables/useChat'
import type { PeerConversationEntry } from '~/composables/useConversations'

type Item =
  | { key: string; at: number; kind: 'agent' }
  | { key: string; at: number; kind: 'notes' }
  | { key: string; at: number; kind: 'peer'; peer: PeerConversationEntry }

const props = defineProps<{
  /** Collapsed to the rail of avatars (the frame decides). */
  collapsed: boolean
  toggleLabel: string
  /** The toggle's `aria-expanded`: the full sidebar is showing. */
  toggleExpanded: boolean
  /** Agent sessions, newest activity first (`list_threads`). */
  threads: Thread[]
  /** True while an agent session is shown. */
  agentActive: boolean
  /** Contact and group conversations and the notes. */
  peers: PeerConversationEntry[]
  /** The "Ich" entry (FR-002b). */
  notes: PeerConversationEntry
  activePeerId: string | null
}>()

const emit = defineEmits<{
  selectAgent: []
  selectNotes: []
  selectPeer: [id: string]
  newConversation: []
  toggle: []
  selectHit: [hit: ChatSearchHit]
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const fieldLabels = useFieldLabels()
// Lives with the sidebar, which stays mounted while it collapses: an expanded sidebar shows the
// typed search again (spec US2 scenario 6). A failed search shows as a toast.
const search = useChatSearch((cause) =>
  toast.error(t('chat.search.error', { error: errString(cause) })),
)

const latestSession = computed(() => props.threads[0] ?? null)

const requests = computed(() =>
  props.peers
    .filter((peer) => peer.state === 'request')
    .sort((a, b) => b.lastAt - a.lastAt),
)
const showingRequests = ref(false)
/** The rail's requests entry: the requests are a list, so the sidebar opens on them. */
function openRequestsFromRail() {
  showingRequests.value = true
  emit('toggle')
}
watch(
  () => requests.value.length,
  (count) => {
    if (count === 0) showingRequests.value = false
  },
)

// FR-001: one entry per counterpart, newest activity first; "Agent" and "Ich" are always there.
const items = computed<Item[]>(() => {
  const list: Item[] = [
    { key: 'agent', at: latestSession.value?.updatedAt ?? 0, kind: 'agent' },
    { key: 'notes', at: props.notes.lastAt, kind: 'notes' },
  ]
  for (const peer of props.peers) {
    if (peer.kind !== 'note' && peer.state !== 'request')
      list.push({ key: `p:${peer.id}`, at: peer.lastAt, kind: 'peer', peer })
  }
  return list.sort((a, b) => b.at - a.at)
})

const searching = computed(() => search.text.value.trim() !== '')

/** Escape clears the typed search; with nothing typed it goes on to the frame, which closes an
 * overlaying sidebar. */
function onSearchEscape(event: KeyboardEvent) {
  if (search.text.value === '') return
  event.preventDefault()
  event.stopPropagation()
  search.clear()
}

function itemName(item: Item): string {
  if (item.kind === 'agent') return t('chat.agent.name')
  if (item.kind === 'notes') return props.notes.displayName
  return item.peer.displayName
}

// FR-010: names narrow the list at once, ignoring case.
const shownItems = computed(() => {
  const needle = search.text.value.trim().toLocaleLowerCase()
  if (!needle) return items.value
  return items.value.filter((item) =>
    itemName(item).toLocaleLowerCase().includes(needle),
  )
})

/** Name and picture of a hit's conversation; agent hits belong to the agent entry. */
function resolveHit(hit: ChatSearchHit) {
  if (hit.kind === 'agent')
    return { name: t('chat.agent.name'), photo: null, isAgent: true }
  const peer =
    props.peers.find((candidate) => candidate.id === hit.threadId) ??
    (props.notes.id === hit.threadId ? props.notes : null)
  return {
    name: peer?.displayName ?? t('chat.peer.unknownName'),
    photo: peer?.photo ?? null,
    isAgent: false,
  }
}
</script>

<template>
  <aside
    class="flex h-full w-full flex-col overflow-hidden border-r border-border bg-background"
    :class="collapsed ? 'items-center gap-2 py-2' : 'min-w-64 gap-3 p-3'"
  >
    <!-- One element for both states, so the toggle keeps its focus while the sidebar collapses. -->
    <div
      class="flex items-center gap-2"
      :class="collapsed ? 'flex-col' : 'w-full min-w-0'"
    >
      <UiButton
        variant="ghost"
        size="icon"
        :aria-label="toggleLabel"
        :tooltip="toggleLabel"
        :aria-expanded="toggleExpanded"
        aria-controls="chat-sidebar"
        data-testid="chat-sidebar-toggle"
        @click="emit('toggle')"
      >
        <Icon name="lucide:panel-left" class="size-4" />
      </UiButton>
      <h2
        v-if="!collapsed"
        class="min-w-0 flex-1 truncate text-sm font-semibold"
      >
        {{ t('chat.sidebar.title') }}
      </h2>
      <UiButton
        variant="ghost"
        size="icon"
        :aria-label="t('chat.list.newConversation')"
        :tooltip="t('chat.list.newConversation')"
        data-testid="chat-new-conversation"
        @click="emit('newConversation')"
      >
        <Icon name="lucide:square-pen" class="size-4" />
      </UiButton>
    </div>

    <nav
      v-if="collapsed"
      class="flex min-h-0 w-full flex-1 flex-col items-center gap-1 overflow-y-auto"
      :aria-label="t('chat.list.title')"
    >
      <ChatRailItem
        v-if="requests.length > 0"
        kind="requests"
        :name="t('chat.list.requests')"
        :active="false"
        :unread="requests.length"
        @select="openRequestsFromRail"
      />
      <template v-for="item in items" :key="item.key">
        <ChatRailItem
          v-if="item.kind === 'agent'"
          kind="agent"
          :name="t('chat.agent.name')"
          :active="agentActive && activePeerId === null"
          :unread="0"
          @select="emit('selectAgent')"
        />
        <ChatRailItem
          v-else-if="item.kind === 'notes'"
          kind="peer"
          :name="notes.displayName"
          :photo="notes.photo"
          :active="notes.id !== '' && activePeerId === notes.id"
          :unread="notes.unread"
          @select="emit('selectNotes')"
        />
        <ChatRailItem
          v-else
          kind="peer"
          :name="item.peer.displayName"
          :photo="item.peer.photo"
          :active="activePeerId === item.peer.id"
          :unread="item.peer.unread"
          @select="emit('selectPeer', item.peer.id)"
        />
      </template>
    </nav>

    <div
      v-else-if="showingRequests"
      class="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto"
    >
      <UiButton
        class="w-full justify-start gap-2"
        variant="ghost"
        data-testid="chat-requests-back"
        @click="showingRequests = false"
      >
        <Icon name="lucide:arrow-left" class="h-4 w-4" />
        {{ t('chat.list.requests') }}
      </UiButton>
      <div class="space-y-1" role="list" :aria-label="t('chat.list.requests')">
        <ChatPeerListItem
          v-for="request in requests"
          :key="request.id"
          role="listitem"
          test-id="chat-request"
          :peer="request"
          :active="activePeerId === request.id"
          @select="emit('selectPeer', request.id)"
        />
      </div>
    </div>

    <div v-else class="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto">
      <UiInput
        v-model="search.text.value"
        type="search"
        :placeholder="t('chat.search.placeholder')"
        :aria-label="t('chat.search.placeholder')"
        :labels="fieldLabels.input.value"
        :prepend-icon="Search"
        clearable
        class="shrink-0 [&_input::-webkit-search-cancel-button]:hidden"
        data-testid="chat-sidebar-search"
        @keydown.esc="onSearchEscape"
      />

      <SidebarRow
        v-if="requests.length > 0 && !searching"
        class="font-medium"
        icon="lucide:inbox"
        icon-class="text-muted-foreground"
        data-testid="chat-requests"
        @click="showingRequests = true"
      >
        {{ t('chat.list.requests') }}
        <template #trailing>
          <span
            class="rounded-full bg-primary px-2 py-0.5 text-xs text-primary-foreground tabular-nums"
            data-testid="chat-requests-count"
          >
            {{ requests.length }}
          </span>
        </template>
      </SidebarRow>

      <div class="space-y-1" role="list" :aria-label="t('chat.list.title')">
        <template v-for="item in shownItems" :key="item.key">
          <ChatAgentListItem
            v-if="item.kind === 'agent'"
            role="listitem"
            :latest="latestSession"
            :active="agentActive && activePeerId === null"
            @select="emit('selectAgent')"
          />
          <ChatPeerListItem
            v-else-if="item.kind === 'notes'"
            role="listitem"
            test-id="chat-notes"
            :peer="notes"
            :active="notes.id !== '' && activePeerId === notes.id"
            @select="emit('selectNotes')"
          />
          <ChatPeerListItem
            v-else
            role="listitem"
            :peer="item.peer"
            :active="activePeerId === item.peer.id"
            @select="emit('selectPeer', item.peer.id)"
          />
        </template>
      </div>
      <p
        v-if="searching && shownItems.length === 0"
        class="px-2 text-xs text-muted-foreground"
      >
        {{ t('chat.search.noNames') }}
      </p>

      <template v-if="searching">
        <ChatSearchHits
          v-if="search.hits.value.length > 0"
          :hits="search.hits.value"
          :more="search.more.value"
          :text="search.text.value"
          :resolve="resolveHit"
          @select="emit('selectHit', $event)"
        />
        <p
          v-else-if="
            !search.failed.value && search.searched.value === search.text.value
          "
          class="px-2 text-xs text-muted-foreground"
        >
          {{ t('chat.search.noMessages') }}
        </p>
      </template>
    </div>
  </aside>
</template>
