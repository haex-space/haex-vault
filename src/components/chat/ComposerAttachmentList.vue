<script setup lang="ts">
/**
 * What is attached to the message being composed (spec 011 FR-013), as chips in the first row of
 * the input box. An image's chip opens its preview (FR-020); the others only show their name. The
 * row sits above the text so the controls below stay where they are, whatever is attached.
 */
import type { ComposerAttachment } from '~/components/chat/ComposerAttachments.vue'

defineProps<{
  attachments: ComposerAttachment[]
}>()

const emit = defineEmits<{
  remove: [id: string]
}>()

const { t } = useI18n()

const previewing = ref<ComposerAttachment | null>(null)
const previewOpen = computed({
  get: () => previewing.value !== null,
  set: (open: boolean) => {
    if (!open) previewing.value = null
  },
})
</script>

<template>
  <div
    class="flex min-w-0 flex-wrap items-center gap-1.5 border-b border-border px-3 pb-3 pt-3"
    data-testid="chat-attachments"
  >
    <div
      v-for="attachment in attachments"
      :key="attachment.id"
      class="flex max-w-full shrink-0 items-center gap-1 rounded-lg border border-border/70 bg-muted/20 py-1 pl-2 pr-1 text-xs"
      :class="{ 'text-destructive': !attachment.info.usable }"
      :title="attachment.info.reason ?? attachment.info.name"
      data-testid="chat-attachment"
    >
      <button
        v-if="attachment.info.kind === 'image'"
        type="button"
        class="max-w-[12rem] truncate rounded outline-none hover:underline focus-visible:ring-2 focus-visible:ring-ring"
        :aria-label="
          t('chat.composer.attachments.preview', { name: attachment.info.name })
        "
        @click="previewing = attachment"
      >
        {{ attachment.info.name }}
      </button>
      <span v-else class="max-w-[12rem] truncate">{{
        attachment.info.name
      }}</span>
      <button
        type="button"
        class="flex h-4 w-4 shrink-0 items-center justify-center rounded-full text-muted-foreground hover:bg-muted hover:text-foreground"
        :aria-label="
          t('chat.composer.attachments.remove', { name: attachment.info.name })
        "
        @click="emit('remove', attachment.id)"
      >
        <Icon name="lucide:x" class="h-3 w-3" :aria-hidden="true" />
      </button>
    </div>
    <ChatComposerAttachmentPreview
      v-model:open="previewOpen"
      :attachment="previewing"
    />
  </div>
</template>
