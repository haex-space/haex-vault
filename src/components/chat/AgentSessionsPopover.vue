<script setup lang="ts">
/**
 * "Verlauf" of the agent (spec 072 FR-002a): every agent session with its title and age, newest
 * first; open, rename and delete with confirmation. Presentational: the state and the actions live
 * in `useThreadSidebar` and the tab actions (`useChatTab`), as before in the thread sidebar.
 */
import {
  nextTick,
  ref,
  shallowRef,
  watch,
  type ComponentPublicInstance,
} from 'vue'
import type { Thread } from '~/composables/useChat'

const props = defineProps<{
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
}>()

const { t } = useI18n()
const open = ref(false)

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
      align="end"
      class="flex max-h-[70vh] w-80 flex-col gap-1 overflow-y-auto p-2"
      data-testid="chat-history-panel"
      :aria-label="t('chat.threads.title')"
    >
      <div class="flex items-center justify-between px-1 pb-1">
        <span class="text-xs font-medium text-muted-foreground">{{
          t('chat.threads.title')
        }}</span>
        <span class="text-[10px] text-muted-foreground">{{
          threads.length
        }}</span>
      </div>
      <div
        v-for="thread in threads"
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
            <Icon name="lucide:check" class="h-4 w-4" />
          </button>
          <button
            type="button"
            class="mr-1 shrink-0 rounded p-1.5 text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            :aria-label="t('chat.threads.cancelEdit')"
            :title="t('chat.threads.cancelEdit')"
            :disabled="renamingThreadId === thread.id"
            @click.stop="emit('cancelEditing')"
          >
            <Icon name="lucide:x" class="h-4 w-4" />
          </button>
        </template>
        <template v-else>
          <button
            type="button"
            class="min-w-0 flex-1 truncate px-2 py-2 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
            @click="select(thread.id)"
          >
            <span class="truncate">{{ thread.title }}</span>
          </button>
          <div
            class="flex max-w-0 shrink-0 items-center overflow-hidden opacity-0 transition-[max-width,opacity] duration-150 group-hover:max-w-14 group-hover:opacity-100 group-focus-within:max-w-14 group-focus-within:opacity-100"
          >
            <button
              type="button"
              class="rounded p-1.5 text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              :aria-label="t('chat.threads.editTitle')"
              :title="t('chat.threads.editTitle')"
              @click.stop="emit('startEditing', thread)"
            >
              <Icon name="lucide:pencil" class="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              class="mr-1 rounded p-1.5 text-muted-foreground hover:text-destructive focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              :aria-label="t('chat.threads.deleteTitle')"
              :title="t('chat.threads.deleteTitle')"
              @click.stop="requestDelete(thread)"
            >
              <Icon name="lucide:trash-2" class="h-3.5 w-3.5" />
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
        v-if="threads.length === 0"
        class="px-3 py-2 text-xs text-muted-foreground"
      >
        {{ t('chat.threads.empty') }}
      </div>
    </ShadcnPopoverContent>
  </ShadcnPopover>

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
