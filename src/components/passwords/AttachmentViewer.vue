<script setup lang="ts">
/**
 * The attachment view (spec 058, US1, FR-003, FR-005, FR-006): one text attachment to read, over the
 * whole password manager window. The text comes from the text reader of the file viewer and shows
 * with the same `FilesTextContent`, so both apps look alike (SC-003). Nothing lands on the device;
 * the text lives in this component and goes with it. Escape and the close button close it; the list
 * gives the focus back to the card. The view has `role="dialog"`, so the list shortcuts leave its
 * keys alone and Ctrl+C copies the selected text. Locking the vault ends the window's process
 * (spec 013), and the view with it (FR-008).
 */
import type { AttachmentView } from '@bindings/AttachmentView'
import type { TextContent } from '@bindings/TextContent'
import { fileKind } from '~/lib/passwords/format'

const props = defineProps<{ attachment: AttachmentView }>()

const emit = defineEmits<{
  close: []
  save: []
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const { attachmentTextAsync } = usePasswords()

const kind = computed(() => fileKind(props.attachment.fileName))
const text = ref<TextContent | null>(null)
const failed = ref<string | null>(null)
const FOCUSABLE_SELECTOR =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'

function focusableElements(): HTMLElement[] {
  return Array.from(
    panel.value?.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR) ?? [],
  ).filter((element) => !element.hidden && element.offsetParent !== null)
}

/** The message for a failure while reading: the reasons of `passwords_attachment_text` in words. */
function failureText(cause: unknown): string {
  if (cause && typeof cause === 'object' && 'kind' in cause) {
    const { kind: errorKind, reason } = cause as {
      kind: unknown
      reason?: unknown
    }
    if (errorKind === 'PasswordsNotFound')
      return t('passwords.attachments.viewer.missing')
    if (errorKind === 'InvalidInput') {
      if (reason === 'binary') return t('passwords.attachments.viewer.binary')
      if (reason === 'not_text')
        return t('passwords.attachments.viewer.notText')
      if (reason === 'unreadable')
        return t('passwords.attachments.viewer.unreadable')
    }
  }
  return errString(cause)
}

watch(
  () => [props.attachment.id, props.attachment.binaryHash] as const,
  async ([id, binaryHash]) => {
    text.value = null
    failed.value = null
    if (kind.value !== 'text') {
      failed.value = t('passwords.attachments.viewer.notText')
      return
    }
    try {
      const content = await attachmentTextAsync(id)
      if (
        id === props.attachment.id &&
        binaryHash === props.attachment.binaryHash
      )
        text.value = content
    } catch (cause) {
      if (
        id === props.attachment.id &&
        binaryHash === props.attachment.binaryHash
      )
        failed.value = failureText(cause)
    }
  },
  { immediate: true },
)

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Tab') {
    const elements = focusableElements()
    if (!elements.length) {
      event.preventDefault()
      panel.value?.focus()
      return
    }
    const index = elements.indexOf(document.activeElement as HTMLElement)
    const next = event.shiftKey
      ? index <= 0
        ? elements[elements.length - 1]
        : elements[index - 1]
      : index < 0 || index === elements.length - 1
        ? elements[0]
        : elements[index + 1]
    if (index < 0 || (event.shiftKey ? index === 0 : index === elements.length - 1)) {
      event.preventDefault()
      next?.focus()
    }
    return
  }
  if (event.key !== 'Escape') return
  event.preventDefault()
  event.stopPropagation()
  emit('close')
}

const panel = useTemplateRef<HTMLElement>('panel')
onMounted(() => panel.value?.focus())
</script>

<template>
  <div
    ref="panel"
    class="absolute inset-0 z-30 flex flex-col bg-background outline-none"
    tabindex="-1"
    role="dialog"
    :aria-label="attachment.fileName"
    data-testid="passwords-attachment-viewer"
    @keydown="onKeydown"
  >
    <div class="flex h-12 shrink-0 items-center gap-1 border-b px-2">
      <UiButton
        type="button"
        variant="ghost"
        size="icon"
        :aria-label="t('passwords.attachments.viewer.close')"
        :tooltip="t('passwords.attachments.viewer.close')"
        data-testid="passwords-attachment-viewer-close"
        @click="emit('close')"
      >
        <Icon name="lucide:x" class="size-4" />
      </UiButton>
      <span class="min-w-0 flex-1 truncate text-sm font-medium">{{
        attachment.fileName
      }}</span>
      <UiButton
        type="button"
        variant="ghost"
        size="icon"
        :aria-label="t('passwords.attachments.download')"
        :tooltip="t('passwords.attachments.download')"
        data-testid="passwords-attachment-viewer-save"
        @click="emit('save')"
      >
        <Icon name="lucide:download" class="size-4" />
      </UiButton>
    </div>

    <div class="relative min-h-0 flex-1 overflow-auto">
      <div
        v-if="failed"
        class="flex flex-col items-center gap-4 p-6 text-center"
        data-testid="passwords-attachment-viewer-failed"
      >
        <p class="text-sm text-muted-foreground">{{ failed }}</p>
        <UiButton type="button" @click="emit('save')">
          <Icon name="lucide:download" class="size-4" />
          {{ t('passwords.attachments.download') }}
        </UiButton>
      </div>
      <div v-else-if="text" data-testid="passwords-attachment-viewer-text">
        <p
          v-if="!text.text && !text.truncated"
          class="p-6 text-center text-sm text-muted-foreground"
        >
          {{ t('passwords.attachments.viewer.empty') }}
        </p>
        <FilesTextContent v-else :text="text" />
      </div>
    </div>
  </div>
</template>
