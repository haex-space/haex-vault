<script setup lang="ts">
/**
 * May an external agent run this call? (spec 021, US3, FR-022, contracts/tauri-commands.md). Shows
 * what the call does (`CallQuestionDetails`) and offers Erlauben and Ablehnen. Prompts of several
 * calls come one after the other, the oldest first. Closing the dialog declines the call. A
 * delegate's call whose chat is open is asked there instead (US6, `chat/DelegateCallPrompt.vue`).
 */
import { toast } from 'vue-sonner'
import type { CallQuestion } from '~/types/bindings/CallQuestion'

const { t } = useI18n()
const { errString } = useErrorString()
const store = useMcpAgentsStore()

const question = computed<CallQuestion | null>(
  () => store.shownCall?.question ?? null,
)
const waiting = computed(() => Math.max(store.outsideCalls.length - 1, 0))

async function answerAsync(allow: boolean) {
  const shown = store.shownCall
  if (!shown) return
  try {
    await store.answerCallAsync(shown.requestId, allow ? 'allow' : 'deny')
  } catch (error) {
    toast.error(errString(error))
  }
}
</script>

<template>
  <UiDrawerModal
    v-if="question"
    :open="true"
    :title="t('mcpAgents.call.title')"
    :dismissible="false"
  >
    <template #content>
      <AgentsCallQuestionDetails
        :question="question"
        :delegate="!!question.delegateThreadId"
      >
        <p v-if="waiting > 0" class="text-xs text-muted-foreground">
          {{ t('mcpAgents.call.more', { count: waiting }) }}
        </p>
      </AgentsCallQuestionDetails>
    </template>
    <template #footer>
      <AgentsCallAnswerButtons
        :destructive="question.effect === 'destructive'"
        @answer="answerAsync"
      />
    </template>
  </UiDrawerModal>
</template>
