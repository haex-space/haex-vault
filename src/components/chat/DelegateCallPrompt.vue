<script setup lang="ts">
/**
 * A delegate's call to holzi's tools, asked in its chat (spec 021 US6, FR-025): where the chat's
 * own delegate approvals appear, not as a separate dialog. While this chat shows the thread, the
 * call dialog leaves its calls to it; closing the chat hands them back. `held` waits while the
 * chat asks its own approval. Closing the prompt declines the call.
 */
import { toast } from 'vue-sonner'

const props = defineProps<{
  threadId: string | null
  held?: boolean
}>()

const { t, locale } = useI18n()
const { errString } = useErrorString()
const store = useMcpAgentsStore()

const call = computed(() =>
  props.threadId && !props.held ? store.callFor(props.threadId) : undefined,
)

const title = computed(() => {
  const asked = call.value?.question
  if (!asked) return ''
  const name =
    (locale.value === 'de' ? asked.titles.de : asked.titles.en) ||
    asked.actionId
  return t('chat.permission.requestTitle', { name })
})

let release: (() => void) | null = null
watch(
  () => props.threadId,
  (threadId) => {
    release?.()
    release = threadId ? store.showThread(threadId) : null
  },
  { immediate: true },
)
onUnmounted(() => release?.())

async function answerAsync(allow: boolean) {
  const shown = call.value
  if (!shown) return
  try {
    await store.answerCallAsync(shown.requestId, allow ? 'allow' : 'deny')
  } catch (error) {
    toast.error(errString(error))
  }
}
</script>

<template>
  <UiDrawerModal v-if="call" :open="true" :title="title" :dismissible="false">
    <template #content>
      <AgentsCallQuestionDetails
        :question="call.question"
        delegate
        data-testid="chat-delegate-call"
      />
    </template>
    <template #footer>
      <AgentsCallAnswerButtons
        :destructive="call.question.effect === 'destructive'"
        @answer="answerAsync"
      />
    </template>
  </UiDrawerModal>
</template>
