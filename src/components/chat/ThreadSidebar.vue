<script setup lang="ts">
/**
 * Thread list, sidebar chrome and the rename/delete UI — extracted from
 * `src/pages/chat/[instance].vue` (spec 015-workspace-shell, T010, plan
 * research R10). Presentational only: all state and the actions it emits
 * live in `useThreadSidebar` (page-level), except the edited-row input's
 * focus/select, which this component owns since it is DOM-local to a row
 * this component renders. Where the sidebar sits and how wide it is, the frame
 * (`apps/ChatApp.vue`, `useChatSidebar`) decides; it fills what it is given and keeps
 * its width while the frame slides it.
 */
import {
  computed,
  nextTick,
  ref,
  shallowRef,
  watch,
  type ComponentPublicInstance,
} from 'vue'
import type { Thread } from '~/composables/useChat'
import type { PeerConversationEntry } from '~/composables/useConversations'

/** Which kinds the list shows (spec 072 FR-002). */
type ListFilter = 'all' | 'agent' | 'contacts' | 'groups'

type Item =
  | { key: string; at: number; thread: Thread; peer?: undefined }
  | { key: string; at: number; peer: PeerConversationEntry; thread?: undefined }

const props = defineProps<{
  instanceName: string
  busy: boolean
  threads: Thread[]
  activeThreadId: string | null
  editingThreadId: string | null
  draftTitle: string
  editTitleError: string | null
  renamingThreadId: string | null
  deleteCandidate: Thread | null
  deleteError: string | null
  deletingThread: boolean
  historyDurationLabel: (createdAt: number) => string
  openingTimeLabel: (createdAt: number) => string
  /** Spec 072: contact and group conversations, listed with the agent's by newest activity. */
  peers: PeerConversationEntry[]
  activePeerId: string | null
}>()

const emit = defineEmits<{
  'update:draftTitle': [value: string]
  newChat: []
  selectThread: [id: string]
  startEditing: [thread: Thread]
  saveTitle: []
  cancelEditing: []
  requestDelete: [thread: Thread]
  closeDeleteDialog: [open: boolean]
  confirmDelete: []
  selectPeer: [id: string]
  writeContact: []
}>()

const { t } = useI18n()

const filter = ref<ListFilter>('all')
const FILTERS: ListFilter[] = ['all', 'agent', 'contacts', 'groups']

// FR-001: one list of every kind, newest activity first; the filter narrows it (FR-002).
const items = computed<Item[]>(() => {
  const list: Item[] = []
  if (filter.value === 'all' || filter.value === 'agent') {
    for (const thread of props.threads)
      list.push({ key: `a:${thread.id}`, at: thread.updatedAt, thread })
  }
  for (const peer of props.peers) {
    const shown =
      filter.value === 'all' ||
      (filter.value === 'contacts' && peer.kind === 'direct') ||
      (filter.value === 'groups' && peer.kind === 'group')
    if (shown) list.push({ key: `p:${peer.id}`, at: peer.lastAt, peer })
  }
  return list.sort((a, b) => b.at - a.at)
})

// `UiInput` wraps the real input, so the ref holds the wrapper element and the
// input is looked up inside it.
const editingTitleField = shallowRef<HTMLElement | null>(null)
function setEditingTitleInput(
  element: Element | ComponentPublicInstance | null,
) {
  editingTitleField.value =
    element === null
      ? null
      : element instanceof HTMLElement
        ? element
        : ((element as ComponentPublicInstance).$el as HTMLElement)
}
watch(
  () => props.editingThreadId,
  (id) => {
    if (id === null) return
    void nextTick(() => {
      const input = editingTitleField.value?.querySelector('input')
      input?.focus()
      input?.select()
    })
  },
)
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

    <UiButton
      class="w-full justify-start gap-2"
      variant="outline"
      :disabled="busy"
      data-testid="chat-new"
      @click="emit('newChat')"
    >
      <Icon name="lucide:plus" class="h-4 w-4" />
      {{ t('chat.newChat') }}
    </UiButton>
    <UiButton
      class="-mt-2 w-full justify-start gap-2"
      variant="outline"
      data-testid="chat-write-contact"
      @click="emit('writeContact')"
    >
      <Icon name="lucide:user-round-plus" class="h-4 w-4" />
      {{ t('chat.list.writeContact') }}
    </UiButton>

    <div
      class="flex flex-wrap gap-1"
      role="group"
      :aria-label="t('chat.list.filter.label')"
    >
      <button
        v-for="option in FILTERS"
        :key="option"
        type="button"
        class="rounded-full border border-border px-2.5 py-0.5 text-xs transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        :class="{ 'bg-accent font-medium': filter === option }"
        :aria-pressed="filter === option"
        :data-testid="`chat-filter-${option}`"
        @click="filter = option"
      >
        {{ t(`chat.list.filter.${option}`) }}
      </button>
    </div>

    <div class="flex items-center justify-between px-1">
      <span class="text-xs font-medium text-muted-foreground">{{
        t('chat.threads.title')
      }}</span>
      <span class="text-[10px] text-muted-foreground">{{ items.length }}</span>
    </div>
    <div class="space-y-1">
      <template v-for="item in items" :key="item.key">
        <div
          v-if="item.thread"
          data-testid="chat-thread"
          class="group flex w-full min-w-0 items-center gap-1 rounded-lg text-sm transition-colors hover:bg-accent focus-within:bg-accent"
          :class="{
            'bg-accent font-medium':
              activePeerId === null && activeThreadId === item.thread.id,
          }"
        >
          <template v-if="editingThreadId === item.thread.id">
            <div class="min-w-0 flex-1 px-2 py-1.5">
              <UiInput
                :ref="setEditingTitleInput"
                :model-value="draftTitle"
                :aria-label="t('chat.threads.editTitle')"
                :disabled="renamingThreadId === item.thread.id"
                @update:model-value="
                  emit('update:draftTitle', String($event ?? ''))
                "
                @keydown.enter.prevent="emit('saveTitle')"
                @keydown.esc.prevent="emit('cancelEditing')"
              />
              <p
                v-if="editTitleError"
                class="mt-1 text-xs text-destructive"
                role="alert"
              >
                {{ editTitleError }}
              </p>
            </div>
            <button
              type="button"
              class="shrink-0 rounded p-1.5 text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              :aria-label="t('chat.threads.saveTitle')"
              :title="t('chat.threads.saveTitle')"
              :disabled="renamingThreadId === item.thread.id"
              @click.stop="emit('saveTitle')"
            >
              <Icon name="lucide:check" class="h-4 w-4" />
            </button>
            <button
              type="button"
              class="mr-1 shrink-0 rounded p-1.5 text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              :aria-label="t('chat.threads.cancelEdit')"
              :title="t('chat.threads.cancelEdit')"
              :disabled="renamingThreadId === item.thread.id"
              @click.stop="emit('cancelEditing')"
            >
              <Icon name="lucide:x" class="h-4 w-4" />
            </button>
          </template>
          <template v-else>
            <button
              type="button"
              class="min-w-0 flex-1 truncate px-2 py-2 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
              @click="emit('selectThread', item.thread.id)"
            >
              <span class="truncate">{{ item.thread.title }}</span>
            </button>
            <div
              class="flex max-w-0 shrink-0 items-center overflow-hidden opacity-0 transition-[max-width,opacity] duration-150 group-hover:max-w-14 group-hover:opacity-100 group-focus-within:max-w-14 group-focus-within:opacity-100"
            >
              <button
                type="button"
                class="rounded p-1.5 text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                :aria-label="t('chat.threads.editTitle')"
                :title="t('chat.threads.editTitle')"
                @click.stop="emit('startEditing', item.thread)"
              >
                <Icon name="lucide:pencil" class="h-3.5 w-3.5" />
              </button>
              <button
                type="button"
                class="mr-1 rounded p-1.5 text-muted-foreground hover:text-destructive focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                :aria-label="t('chat.threads.deleteTitle')"
                :title="t('chat.threads.deleteTitle')"
                @click.stop="emit('requestDelete', item.thread)"
              >
                <Icon name="lucide:trash-2" class="h-3.5 w-3.5" />
              </button>
            </div>
            <span
              class="w-8 shrink-0 rounded pr-1 text-right text-xs font-normal tabular-nums text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              tabindex="0"
              :title="openingTimeLabel(item.thread.createdAt)"
              :aria-label="`${t('chat.threads.duration', { duration: historyDurationLabel(item.thread.createdAt) })}, ${openingTimeLabel(item.thread.createdAt)}`"
            >
              {{ historyDurationLabel(item.thread.createdAt) }}
            </span>
          </template>
        </div>
        <ChatPeerListItem
          v-else-if="item.peer"
          :peer="item.peer"
          :active="activePeerId === item.peer.id"
          @select="emit('selectPeer', item.peer.id)"
        />
      </template>
      <div
        v-if="items.length === 0"
        class="px-3 py-2 text-xs text-muted-foreground"
      >
        {{ t('chat.list.empty') }}
      </div>
    </div>
  </aside>

  <UiDrawerModal
    v-if="deleteCandidate"
    :open="deleteCandidate !== null"
    :title="t('chat.threads.deleteDialogTitle')"
    @update:open="emit('closeDeleteDialog', $event)"
  >
    <template #content>
      <div class="space-y-3 px-6 py-2">
        <p class="text-sm">
          {{
            t('chat.threads.deleteConfirm', { title: deleteCandidate.title })
          }}
        </p>
        <p v-if="deleteError" class="text-sm text-destructive" role="alert">
          {{ deleteError }}
        </p>
      </div>
    </template>
    <template #footer>
      <div class="flex justify-end gap-2">
        <UiButton
          type="button"
          variant="outline"
          :disabled="deletingThread"
          @click="emit('closeDeleteDialog', false)"
        >
          {{ t('chat.cancel') }}
        </UiButton>
        <UiButton
          type="button"
          variant="destructive"
          :loading="deletingThread"
          @click="emit('confirmDelete')"
        >
          {{ t('chat.threads.deleteConfirmButton') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
