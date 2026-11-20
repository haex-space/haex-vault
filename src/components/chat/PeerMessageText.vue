<script setup lang="ts">
/**
 * The text of a message in a contact conversation or a note (spec 072 FR-012a): shown folded to
 * about twelve lines with "Weiterlesen" and unfolded by a click. A long text that travelled as a
 * file has its preview as `text` and always offers "Weiterlesen": the click reads the full text
 * from this device's copy of the file, which the sender has at once and the receiver downloads by
 * itself. While the copy is coming the button shows the progress; when the download failed it can be
 * tried again; when the file is gone from the server the preview stays with a hint.
 */
import type { ChatPeerAttachment } from '@bindings/ChatPeerAttachment'
import { textControl } from '~/lib/chat/longText'
import { formatFileSize } from '~/lib/passwords/format'

const props = defineProps<{
  text: string
  /** The file that holds the full text, when the text is only a preview. */
  file: ChatPeerAttachment | null
}>()

const emit = defineEmits<{
  /** Fetch the file of this attachment now (retry, or above the automatic limit). */
  download: [attachmentId: string]
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const commands = useConversationCommands()

const expanded = ref(false)
const full = ref<string | null>(null)
const reading = ref(false)
const failure = ref<string | null>(null)
const paragraph = ref<HTMLElement | null>(null)
/** The folded text is cut off (a text of its own, without a file). */
const overflows = ref(false)

const control = computed(() => (props.file ? textControl(props.file) : null))
const shown = computed(() =>
  expanded.value && full.value !== null ? full.value : props.text,
)
/** "Weiterlesen" or "Weniger anzeigen" is offered. */
const toggles = computed(() =>
  props.file
    ? control.value?.kind === 'toggle'
    : overflows.value || expanded.value,
)

function measure() {
  const element = paragraph.value
  overflows.value =
    element !== null &&
    !expanded.value &&
    element.scrollHeight > element.clientHeight + 1
}

let observer: ResizeObserver | null = null
onMounted(() => {
  measure()
  if (paragraph.value && typeof ResizeObserver !== 'undefined') {
    observer = new ResizeObserver(measure)
    observer.observe(paragraph.value)
  }
})
onBeforeUnmount(() => observer?.disconnect())
watch(
  () => props.text,
  () => nextTick(measure),
)

async function toggle() {
  if (expanded.value) {
    expanded.value = false
    await nextTick()
    measure()
    return
  }
  const file = props.file
  if (file && full.value === null) {
    reading.value = true
    failure.value = null
    try {
      full.value = await commands.textAsync(file.id)
    } catch (e: unknown) {
      failure.value = errString(e)
      return
    } finally {
      reading.value = false
    }
  }
  expanded.value = true
}
</script>

<template>
  <div class="flex flex-col gap-1">
    <p
      v-if="shown"
      ref="paragraph"
      class="whitespace-pre-wrap break-words text-sm"
      :class="expanded ? '' : 'line-clamp-12'"
      data-testid="chat-peer-text"
    >
      {{ shown }}
    </p>
    <template v-if="file && control">
      <button
        v-if="control.kind === 'toggle'"
        type="button"
        class="self-start text-xs font-medium underline underline-offset-2 disabled:opacity-60"
        :disabled="reading"
        data-testid="chat-peer-read-more"
        @click="toggle"
      >
        {{
          expanded
            ? t('chat.peer.readLess')
            : t('chat.peer.readMoreSize', { size: formatFileSize(file.size) })
        }}
      </button>
      <span
        v-else-if="control.kind === 'loading'"
        class="text-xs opacity-80"
        data-testid="chat-peer-text-loading"
      >
        {{
          control.percent === null
            ? t('chat.peer.textLoading')
            : t('chat.peer.textLoadingPercent', { percent: control.percent })
        }}
      </span>
      <button
        v-else-if="control.kind === 'download' || control.kind === 'retry'"
        type="button"
        class="self-start text-xs font-medium underline underline-offset-2"
        data-testid="chat-peer-text-download"
        @click="emit('download', file.id)"
      >
        {{
          control.kind === 'retry'
            ? t('chat.peer.textRetry')
            : t('chat.peer.textDownload', { size: formatFileSize(file.size) })
        }}
      </button>
      <span v-else class="text-xs opacity-80" data-testid="chat-peer-text-gone">
        {{ t('chat.peer.textGone') }}
      </span>
    </template>
    <button
      v-else-if="toggles"
      type="button"
      class="self-start text-xs font-medium underline underline-offset-2"
      data-testid="chat-peer-read-more"
      @click="toggle"
    >
      {{ expanded ? t('chat.peer.readLess') : t('chat.peer.readMore') }}
    </button>
    <p
      v-if="failure"
      class="text-xs text-destructive"
      data-testid="chat-peer-text-error"
    >
      {{ failure }}
    </p>
  </div>
</template>
