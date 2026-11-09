<script setup lang="ts">
/**
 * The actions under an answer of the agent (spec 066 FR-022c, operator 2026-11-07): read it aloud
 * (the button stops it while it plays) and copy its text. Copying shows a check for a moment
 * instead of a notice; errors come as a toast.
 */
import { toast } from 'vue-sonner'

const props = defineProps<{
  messageId: string
  /** The answer as the model wrote it (Markdown). */
  content: string
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const { speakingId, toggleAsync } = useMessageSpeech()

const speaking = computed(() => speakingId.value === props.messageId)
const copied = ref(false)
let copiedTimer: ReturnType<typeof setTimeout> | undefined

async function speakAsync() {
  try {
    await toggleAsync(props.messageId, props.content)
  } catch (e) {
    toast.error(t('chat.messageActions.speakFailed', { reason: errString(e) }))
  }
}

async function copyAsync() {
  try {
    await navigator.clipboard.writeText(props.content)
    copied.value = true
    clearTimeout(copiedTimer)
    copiedTimer = setTimeout(() => (copied.value = false), 1500)
  } catch {
    toast.error(t('chat.messageActions.copyFailed'))
  }
}

onBeforeUnmount(() => clearTimeout(copiedTimer))
</script>

<template>
  <div class="mt-1 flex items-center gap-1 text-muted-foreground">
    <UiButton
      type="button"
      variant="ghost"
      size="icon-sm"
      :aria-label="
        speaking
          ? t('chat.messageActions.stop')
          : t('chat.messageActions.speak')
      "
      :tooltip="
        speaking
          ? t('chat.messageActions.stop')
          : t('chat.messageActions.speak')
      "
      :aria-pressed="speaking"
      data-testid="chat-message-speak"
      @click="speakAsync"
    >
      <Icon
        :name="speaking ? 'lucide:square' : 'lucide:volume-2'"
        class="size-4"
      />
    </UiButton>
    <UiButton
      type="button"
      variant="ghost"
      size="icon-sm"
      :aria-label="
        copied ? t('chat.messageActions.copied') : t('chat.messageActions.copy')
      "
      :tooltip="
        copied ? t('chat.messageActions.copied') : t('chat.messageActions.copy')
      "
      data-testid="chat-message-copy"
      @click="copyAsync"
    >
      <Icon :name="copied ? 'lucide:check' : 'lucide:copy'" class="size-4" />
    </UiButton>
  </div>
</template>
