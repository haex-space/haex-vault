<script setup lang="ts">
/**
 * A file of a message over the conversation (spec 072 US3 scenario 4), shown as the file browser
 * shows it: text read here, an image, video, audio or PDF streamed from this device's encrypted
 * copy through the media server (the URL is released when the view closes), all in
 * `FilesViewerContent`. Escape and the close button close it; "Speichern unter" and "Mit System-App
 * öffnen" work on the same file.
 */
import type { ChatPeerAttachment } from '@bindings/ChatPeerAttachment'
import type { TextContent } from '@bindings/TextContent'
import { viewerKind } from '~/lib/files/viewerKind'

const props = defineProps<{ file: ChatPeerAttachment }>()

const emit = defineEmits<{
  close: []
  save: [file: ChatPeerAttachment]
  openSystem: [attachmentId: string]
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const commands = useConversationCommands()
const { releaseAsync } = useFiles()
const { isAndroid } = useDeviceCapabilities()
const { tabId } = useWmTab()

const name = computed(() => props.file.name ?? t('chat.file.unnamed'))
const kind = computed(() => viewerKind(name.value))
/** Set when the web view could not play or render the file. */
const unplayable = ref(false)
const shown = computed(() =>
  unplayable.value || kind.value === 'info' ? null : kind.value,
)
const text = ref<TextContent | null>(null)
const url = ref<string | null>(null)
const failed = ref<string | null>(null)

function dropUrl() {
  const current = url.value
  url.value = null
  if (current) void releaseAsync(current).catch(() => {})
}

/** The reasons of `chat_attachment_read_text` in words; anything else as usual. */
function failureText(cause: unknown): string {
  const reason = (cause as { reason?: unknown } | null)?.reason
  if (reason === 'binary') return t('chat.file.viewer.binary')
  return errString(cause)
}

watch(
  () => props.file.id,
  async (id) => {
    text.value = null
    dropUrl()
    failed.value = null
    unplayable.value = false
    try {
      if (kind.value === 'text') {
        const content = await commands.readTextAsync(id)
        if (id === props.file.id) text.value = content
      } else if (kind.value !== 'info') {
        const opened = await commands.openAsync(id, tabId)
        if (id === props.file.id) url.value = opened.url
        else if (opened.url) void releaseAsync(opened.url).catch(() => {})
      }
    } catch (cause) {
      if (id === props.file.id) failed.value = failureText(cause)
    }
  },
  { immediate: true },
)
onBeforeUnmount(dropUrl)

const panel = useTemplateRef<HTMLElement>('panel')
onMounted(() => panel.value?.focus())
</script>

<template>
  <div
    ref="panel"
    class="absolute inset-0 z-30 flex flex-col bg-background outline-none"
    tabindex="-1"
    role="dialog"
    :aria-label="name"
    data-testid="chat-attachment-viewer"
    @keydown.esc="emit('close')"
  >
    <div class="flex h-12 shrink-0 items-center gap-1 border-b px-2">
      <UiButtonClose
        :label="t('files.viewer.close')"
        data-testid="chat-attachment-viewer-close"
        @click="emit('close')"
      />
      <span class="min-w-0 flex-1 truncate text-sm font-medium">{{
        name
      }}</span>
      <UiButton
        variant="ghost"
        size="icon"
        :aria-label="t('chat.file.save')"
        :tooltip="t('chat.file.save')"
        data-testid="chat-attachment-viewer-save"
        @click="emit('save', file)"
      >
        <Icon name="lucide:save" class="size-4" />
      </UiButton>
      <UiButtonOpenExternal
        v-if="!isAndroid"
        :label="t('chat.file.openSystem')"
        data-testid="chat-attachment-viewer-open-system"
        @click="emit('openSystem', file.id)"
      />
    </div>
    <div class="relative min-h-0 flex-1 overflow-auto">
      <p
        v-if="failed"
        class="p-6 text-center text-sm text-muted-foreground"
        data-testid="chat-attachment-viewer-failed"
      >
        {{ failed }}
      </p>
      <FilesViewerContent
        v-else-if="shown"
        :kind="shown"
        :text="text"
        :url="url"
        :name="name"
        @failed="unplayable = true"
      />
      <p
        v-if="unplayable"
        class="border-b bg-muted px-4 py-2 text-center text-xs text-muted-foreground"
        data-testid="chat-attachment-viewer-unplayable"
      >
        {{ t('files.viewer.unplayable') }}
      </p>
    </div>
  </div>
</template>
