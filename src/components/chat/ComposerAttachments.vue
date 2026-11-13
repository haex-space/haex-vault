<script setup lang="ts">
import type { AttachmentInfo } from '~/composables/useChat'

export interface ComposerAttachment {
  id: string
  /** The choice of the open dialog (spec 043: a path or an address). */
  path: string
  info: AttachmentInfo
}

/** The "+" control; the chips of what is attached are `ComposerAttachmentList.vue`, above the
 * text field, so this row never changes. */

/** Extensions `chat/attachments.rs::kind_and_media_type` actually
 * recognizes — kept in sync manually, same as any other UI-side mirror of
 * a backend capability list in this composer. */
const SUPPORTED_EXTENSIONS = [
  'png',
  'jpg',
  'jpeg',
  'webp',
  'gif',
  'pdf',
  'txt',
  'md',
]

const props = defineProps<{
  disabled?: boolean
}>()

const emit = defineEmits<{
  add: [paths: string[]]
}>()

const { t } = useI18n()
const { pickManyAsync } = usePickedFile()

async function pickFiles() {
  if (props.disabled) return
  const selected = await pickManyAsync([
    {
      name: t('chat.composer.attachments.filterName'),
      extensions: SUPPORTED_EXTENSIONS,
    },
  ])
  if (selected.length) emit('add', selected)
}
</script>

<template>
  <div class="flex min-w-0 shrink-0 items-center">
    <button
      type="button"
      class="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg text-muted-foreground outline-none transition-colors hover:bg-muted/60 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50"
      :disabled="disabled"
      :aria-label="t('chat.composer.attachments.attachButton')"
      :title="t('chat.composer.attachments.attachButton')"
      @click="pickFiles"
    >
      <Icon name="lucide:plus" class="h-4 w-4" :aria-hidden="true" />
    </button>
  </div>
</template>
