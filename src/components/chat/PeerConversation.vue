<script setup lang="ts">
/**
 * A contact or group conversation (spec 072 US1, FR-006): header with picture and name, the
 * messages and a composer with text and dictation, without any model, effort, permission or agent
 * control. Sending is wired with spec 072 US2; until then the composer explains that it comes next.
 */
import type { PeerConversationEntry } from '~/composables/useConversations'

const props = defineProps<{
  conversation: PeerConversationEntry | null
}>()

const emit = defineEmits<{
  mute: [threadId: string, muted: boolean]
  delete: [threadId: string]
}>()

const { t } = useI18n()
const draft = ref('')
const confirmDelete = ref(false)

function deleteConversation() {
  const threadId = props.conversation?.id
  confirmDelete.value = false
  if (threadId) emit('delete', threadId)
}

watch(
  () => props.conversation?.id,
  () => {
    draft.value = ''
    confirmDelete.value = false
  },
)
</script>

<template>
  <section
    class="flex min-h-0 min-w-0 flex-1 flex-col"
    data-testid="chat-peer-conversation"
  >
    <template v-if="conversation">
      <header
        class="flex items-center gap-3 border-b border-border px-4 py-3 md:px-8"
      >
        <ContactsAvatar
          :name="conversation.displayName"
          :photo="conversation.photo"
        />
        <div class="min-w-0 flex-1">
          <h2 class="truncate text-base font-semibold">
            {{ conversation.displayName }}
          </h2>
          <p
            v-if="conversation.muted"
            class="flex items-center gap-1 text-xs text-muted-foreground"
          >
            <Icon name="lucide:bell-off" class="h-3 w-3" />
            {{ t('chat.list.muted') }}
          </p>
        </div>
        <UiButton
          type="button"
          variant="ghost"
          size="icon"
          :aria-label="
            conversation.muted ? t('chat.peer.unmute') : t('chat.peer.mute')
          "
          :tooltip="
            conversation.muted ? t('chat.peer.unmute') : t('chat.peer.mute')
          "
          data-testid="chat-peer-mute"
          @click="emit('mute', conversation.id, !conversation.muted)"
        >
          <Icon
            :name="conversation.muted ? 'lucide:bell' : 'lucide:bell-off'"
            class="h-4 w-4"
          />
        </UiButton>
        <UiButton
          type="button"
          variant="ghost"
          size="icon"
          :aria-label="t('chat.peer.delete')"
          :tooltip="t('chat.peer.delete')"
          data-testid="chat-peer-delete"
          @click="confirmDelete = true"
        >
          <Icon name="lucide:trash-2" class="h-4 w-4" />
        </UiButton>
      </header>

      <div
        class="flex min-h-0 flex-1 flex-col overflow-y-auto px-4 py-6 md:px-8"
      >
        <p class="m-auto text-sm text-muted-foreground">
          {{ t('chat.peer.noMessages') }}
        </p>
      </div>

      <div class="border-t border-border px-4 py-3 md:px-8">
        <UiTextarea
          v-model="draft"
          :placeholder="t('chat.peer.placeholder')"
          :aria-label="t('chat.peer.placeholder')"
          disabled
          rows="2"
        />
        <p class="mt-2 text-xs text-muted-foreground">
          {{ t('chat.peer.sendingLater') }}
        </p>
      </div>
    </template>
    <p v-else class="m-auto text-sm text-muted-foreground">
      {{ t('chat.peer.missing') }}
    </p>

    <UiDrawerModal
      v-if="conversation && confirmDelete"
      :open="confirmDelete"
      :title="t('chat.peer.delete')"
      @update:open="confirmDelete = $event"
    >
      <template #content>
        <p class="px-6 py-2 text-sm">
          {{ t('chat.peer.deleteConfirm', { name: conversation.displayName }) }}
        </p>
      </template>
      <template #footer>
        <div class="flex justify-end gap-2">
          <UiButton
            type="button"
            variant="outline"
            @click="confirmDelete = false"
          >
            {{ t('chat.cancel') }}
          </UiButton>
          <UiButton
            type="button"
            variant="destructive"
            data-testid="chat-peer-delete-confirm"
            @click="deleteConversation"
          >
            {{ t('chat.peer.deleteButton') }}
          </UiButton>
        </div>
      </template>
    </UiDrawerModal>
  </section>
</template>
