<script setup lang="ts">
/**
 * The chat's list, like a messenger's (spec 072 FR-001, FR-002, FR-002b, revision 2026-11-01): one
 * entry per contact and group, exactly one "Agent" entry and one "Ich" entry for the notes, newest
 * activity first. The agent's sessions are not listed here; its header offers them ("Verlauf").
 * Requests (FR-002, FR-040) are not in the list either: an entry "Anfragen" with their count leads
 * to them, as a list of its own in the sidebar.
 * Where the sidebar sits and how wide it is, the frame (`apps/ChatApp.vue`, `useChatSidebar`)
 * decides; it fills what it is given and keeps its width while the frame slides it.
 */
import type { Thread } from '~/composables/useChat'
import type { PeerConversationEntry } from '~/composables/useConversations'

type Item =
  | { key: string; at: number; kind: 'agent' }
  | { key: string; at: number; kind: 'notes' }
  | { key: string; at: number; kind: 'peer'; peer: PeerConversationEntry }

const props = defineProps<{
  instanceName: string
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
}>()

const { t } = useI18n()

const latestSession = computed(() => props.threads[0] ?? null)

const requests = computed(() =>
  props.peers
    .filter((peer) => peer.state === 'request')
    .sort((a, b) => b.lastAt - a.lastAt),
)
const showingRequests = ref(false)
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
</script>

<template>
  <aside
    class="flex h-full w-full min-w-64 flex-col gap-4 overflow-y-auto border-r border-border bg-background p-4"
  >
    <div class="flex items-center gap-3 min-w-0">
      <div
        class="h-9 w-9 shrink-0 rounded-xl bg-foreground text-background flex items-center justify-center"
      >
        <Icon name="lucide:sparkles" class="h-4 w-4" />
      </div>
      <div class="min-w-0">
        <div class="text-sm font-semibold">Holzi</div>
        <div
          class="text-xs text-muted-foreground truncate"
          :title="instanceName"
        >
          {{ instanceName }}
        </div>
      </div>
    </div>

    <template v-if="showingRequests">
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
    </template>

    <template v-else>
      <UiButton
        class="w-full justify-start gap-2"
        variant="outline"
        data-testid="chat-new-conversation"
        @click="emit('newConversation')"
      >
        <Icon name="lucide:square-pen" class="h-4 w-4" />
        {{ t('chat.list.newConversation') }}
      </UiButton>

      <button
        v-if="requests.length > 0"
        type="button"
        class="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left text-sm hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        data-testid="chat-requests"
        @click="showingRequests = true"
      >
        <Icon name="lucide:inbox" class="h-4 w-4 text-muted-foreground" />
        <span class="flex-1 font-medium">{{ t('chat.list.requests') }}</span>
        <span
          class="rounded-full bg-primary px-2 py-0.5 text-xs text-primary-foreground tabular-nums"
          data-testid="chat-requests-count"
        >
          {{ requests.length }}
        </span>
      </button>

      <div class="space-y-1" role="list" :aria-label="t('chat.list.title')">
        <template v-for="item in items" :key="item.key">
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
    </template>
  </aside>
</template>
