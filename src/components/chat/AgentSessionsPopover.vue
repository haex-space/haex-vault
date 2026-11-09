<script setup lang="ts">
/**
 * "Verlauf" of the agent (spec 072 FR-002a): every agent session with its title and age, newest
 * first; open, rename and delete with confirmation. Presentational: the state and the actions live
 * in `useThreadSidebar` and the tab actions (`useChatTab`), as before in the thread sidebar.
 * Its search field searches the transcripts of spoken sessions (spec 066 T127); a hit opens the
 * session's transcript at that entry. On a small screen the list is a drawer instead of a popover:
 * the popover floats next to its button and jumped around once the keyboard opened for the search
 * field (as the composer's model select did).
 */
import {
  computed,
  nextTick,
  ref,
  shallowRef,
  watch,
  type ComponentPublicInstance,
} from 'vue'
import { createReusableTemplate } from '@vueuse/core'
import { toast } from 'vue-sonner'
import type { Thread } from '~/composables/useChat'
import type { TranscriptHit } from '~/lib/assistant/transcript'
import { VOICE_THREAD_ICON } from '~/lib/chat/threadOrigin'

const props = defineProps<{
  threads: Thread[]
  activeThreadId: string | null
  editingThreadId: string | null
  draftTitle: string
  editTitleError: string | null
  renameError: string | null
  renamingThreadId: string | null
  deleteCandidate: Thread | null
  deleteError: string | null
  deletingThread: boolean
  historyDurationLabel: (createdAt: number) => string
  openingTimeLabel: (createdAt: number) => string
}>()

const emit = defineEmits<{
  'update:draftTitle': [value: string]
  select: [id: string]
  startEditing: [thread: Thread]
  saveTitle: []
  cancelEditing: []
  requestDelete: [thread: Thread]
  closeDeleteDialog: [open: boolean]
  confirmDelete: []
  openTranscript: [hit: { threadId: string; id: string }]
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const open = ref(false)
const wm = useWindowManagerStore()
const compact = computed(() => wm.compact)
/** The list is written once and shown in the popover or, on a small screen, in the drawer. */
const [DefineList, ReuseList] = createReusableTemplate()
const search = useTranscriptSearch()
const searchActive = computed(() => search.query.value.trim() !== '')
// A spoken entry stored while the popover was closed shows up when it opens again.
watch(open, (isOpen) => {
  if (isOpen && searchActive.value) void search.refreshAsync()
})
// Errors are toasts, never inline text.
watch(search.error, (failure) => {
  if (failure !== null) toast.error(errString(failure))
})

function threadTitle(threadId: string): string {
  return (
    props.threads.find((thread) => thread.id === threadId)?.title ??
    t('chat.agent.unnamedSession')
  )
}

function openHit(hit: TranscriptHit) {
  open.value = false
  emit('openTranscript', { threadId: hit.threadId, id: hit.id })
}

// A failed rename or delete shows as a toast; the title checks stay at the title field.
watch(
  () => props.renameError,
  (message) => {
    if (message) toast.error(message)
  },
)
watch(
  () => props.deleteError,
  (message) => {
    if (message) toast.error(message)
  },
)

function select(id: string) {
  open.value = false
  emit('select', id)
}

function requestDelete(thread: Thread) {
  open.value = false
  emit('requestDelete', thread)
}

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
  <DefineList>
    <div class="flex items-center justify-between px-1 pb-1">
      <span class="text-xs font-medium text-muted-foreground">{{
        t('chat.threads.title')
      }}</span>
      <span class="text-[10px] text-muted-foreground">{{
        threads.length
      }}</span>
    </div>
    <UiInput
      v-model="search.query.value"
      type="search"
      class="mb-1"
      data-testid="chat-transcript-search"
      :placeholder="t('chat.threads.searchTranscripts')"
      :aria-label="t('chat.threads.searchTranscripts')"
    />
    <template v-if="searchActive">
      <button
        v-for="hit in search.hits.value"
        :key="hit.id"
        type="button"
        class="flex w-full min-w-0 flex-col gap-0.5 rounded-lg px-2 py-2 text-left text-sm hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        data-testid="chat-transcript-hit"
        @click="openHit(hit)"
      >
        <span class="truncate text-xs font-medium text-muted-foreground">
          {{ threadTitle(hit.threadId) }}
        </span>
        <span class="line-clamp-2 break-words">{{ hit.snippet }}</span>
      </button>
      <p
        v-if="
          !search.error.value &&
          !search.searching.value &&
          search.hits.value.length === 0
        "
        class="px-3 py-2 text-xs text-muted-foreground"
      >
        {{ t('chat.threads.noHits') }}
      </p>
      <UiButton
        v-if="search.nextCursor.value"
        type="button"
        variant="ghost"
        size="sm"
        :loading="search.searching.value"
        data-testid="chat-transcript-more"
        @click="search.moreAsync()"
      >
        {{ t('chat.threads.moreHits') }}
      </UiButton>
    </template>
    <div
      v-for="thread in searchActive ? [] : threads"
      :key="thread.id"
      data-testid="chat-thread"
      class="group flex w-full min-w-0 items-center gap-1 rounded-lg text-sm transition-colors hover:bg-accent focus-within:bg-accent"
      :class="{ 'bg-accent font-medium': activeThreadId === thread.id }"
    >
      <template v-if="editingThreadId === thread.id">
        <div class="min-w-0 flex-1 px-2 py-1.5">
          <UiInput
            :ref="setEditingTitleInput"
            :model-value="draftTitle"
            :aria-label="t('chat.threads.editTitle')"
            :disabled="renamingThreadId === thread.id"
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
          :disabled="renamingThreadId === thread.id"
          @click.stop="emit('saveTitle')"
        >
          <Icon name="lucide:check" class="h-4 w-4 max-md:h-5 max-md:w-5" />
        </button>
        <button
          type="button"
          class="mr-1 shrink-0 rounded p-1.5 text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          :aria-label="t('chat.threads.cancelEdit')"
          :title="t('chat.threads.cancelEdit')"
          :disabled="renamingThreadId === thread.id"
          @click.stop="emit('cancelEditing')"
        >
          <Icon name="lucide:x" class="h-4 w-4 max-md:h-5 max-md:w-5" />
        </button>
      </template>
      <template v-else>
        <button
          type="button"
          class="min-w-0 flex-1 truncate px-2 py-2 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
          @click="select(thread.id)"
        >
          <Icon
            v-if="thread.origin === 'voice'"
            :name="VOICE_THREAD_ICON"
            class="mr-1.5 inline h-3.5 w-3.5 align-[-2px] text-muted-foreground"
            data-testid="chat-thread-voice"
            :title="t('chat.threads.voice')"
            :aria-label="t('chat.threads.voice')"
          />
          <span class="truncate">{{ thread.title }}</span>
        </button>
        <div
          class="flex max-w-0 shrink-0 items-center overflow-hidden opacity-0 transition-[max-width,opacity] duration-150 group-hover:max-w-14 group-hover:opacity-100 group-focus-within:max-w-14 group-focus-within:opacity-100 max-md:max-w-none max-md:opacity-100"
        >
          <button
            type="button"
            class="rounded p-1.5 text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            :aria-label="t('chat.threads.editTitle')"
            :title="t('chat.threads.editTitle')"
            @click.stop="emit('startEditing', thread)"
          >
            <Icon
              name="lucide:pencil"
              class="h-3.5 w-3.5 max-md:h-5 max-md:w-5"
            />
          </button>
          <button
            type="button"
            class="mr-1 rounded p-1.5 text-muted-foreground hover:text-destructive focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            :aria-label="t('chat.threads.deleteTitle')"
            :title="t('chat.threads.deleteTitle')"
            @click.stop="requestDelete(thread)"
          >
            <Icon
              name="lucide:trash-2"
              class="h-3.5 w-3.5 max-md:h-5 max-md:w-5"
            />
          </button>
        </div>
        <span
          class="w-8 shrink-0 rounded pr-1 text-right text-xs font-normal tabular-nums text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          tabindex="0"
          :title="openingTimeLabel(thread.createdAt)"
          :aria-label="`${t('chat.threads.duration', { duration: historyDurationLabel(thread.createdAt) })}, ${openingTimeLabel(thread.createdAt)}`"
        >
          {{ historyDurationLabel(thread.createdAt) }}
        </span>
      </template>
    </div>
    <div
      v-if="!searchActive && threads.length === 0"
      class="px-3 py-2 text-xs text-muted-foreground"
    >
      {{ t('chat.threads.empty') }}
    </div>
  </DefineList>

  <ShadcnPopover v-model:open="open">
    <ShadcnPopoverTrigger as-child>
      <UiButton
        variant="ghost"
        size="sm"
        class="gap-2"
        data-testid="chat-history"
        :aria-label="t('chat.agent.history')"
      >
        <Icon name="lucide:history" class="h-4 w-4" />
        <span class="hidden @md:inline">{{ t('chat.agent.history') }}</span>
      </UiButton>
    </ShadcnPopoverTrigger>
    <ShadcnPopoverContent
      v-if="!compact"
      align="end"
      class="flex max-h-[70vh] w-80 flex-col gap-1 overflow-y-auto p-2"
      data-testid="chat-history-panel"
      :aria-label="t('chat.threads.title')"
    >
      <ReuseList />
    </ShadcnPopoverContent>
  </ShadcnPopover>

  <UiDrawerModal
    v-if="compact"
    v-model:open="open"
    :title="t('chat.threads.title')"
  >
    <template #content>
      <div class="flex flex-col gap-1" data-testid="chat-history-panel">
        <ReuseList />
      </div>
    </template>
  </UiDrawerModal>

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
