<script setup lang="ts">
/**
 * The agent's question (spec 046, FR-006): the candidates as a radio list, "something else" with a
 * text field, confirm and cancel. Closing the dialog cancels the question (the turn goes on with a
 * declined answer), unlike the approval dialog, where closing stops the turn. A candidate that
 * cannot be picked right now is shown disabled with its reason.
 */
import type { ChoiceAnswer, PendingChoice } from '~/lib/chat/prompts'

const props = defineProps<{
  prompt: PendingChoice
}>()

const emit = defineEmits<{
  answer: [requestId: string, answer: ChoiceAnswer]
}>()

const { t } = useI18n()
const fieldLabels = useFieldLabels()

/** The value of "something else"; an option's value is an app id or the agent's text. */
const OTHER = '__holzi_other_answer__'
const selected = ref<string | null>(null)
const otherText = ref('')

watch(
  () => props.prompt.requestId,
  () => {
    selected.value = null
    otherText.value = ''
  },
)

const title = computed(
  () =>
    props.prompt.question ??
    t('chat.choice.title', { value: props.prompt.value }),
)
const canConfirm = computed(() =>
  selected.value === OTHER
    ? otherText.value.trim() !== ''
    : selected.value !== null,
)

function confirm() {
  if (!canConfirm.value || selected.value === null) return
  emit(
    'answer',
    props.prompt.requestId,
    selected.value === OTHER
      ? { kind: 'text', text: otherText.value.trim() }
      : { kind: 'option', value: selected.value },
  )
}

function cancel() {
  emit('answer', props.prompt.requestId, { kind: 'cancel' })
}
</script>

<template>
  <UiDrawerModal :open="true" :title="title" :dismissible="false">
    <template #content>
      <div class="space-y-3">
        <p
          v-if="prompt.options.length === 0"
          class="text-sm text-muted-foreground"
        >
          {{ t('chat.choice.noCandidates') }}
        </p>
        <ShadcnRadioGroup v-model="selected" class="gap-2" :aria-label="title">
          <label
            v-for="option in prompt.options"
            :key="option.value"
            class="flex items-start gap-2 text-sm"
            :class="option.unavailable ? 'opacity-60' : 'cursor-pointer'"
          >
            <ShadcnRadioGroupItem
              :value="option.value"
              :disabled="Boolean(option.unavailable)"
              class="mt-0.5"
              :data-testid="`chat-choice-option-${option.value}`"
            />
            <span class="flex flex-col">
              <span>{{ option.label }}</span>
              <span
                v-if="option.unavailable"
                class="text-xs text-muted-foreground"
              >
                {{
                  t('chat.choice.unavailable', { reason: option.unavailable })
                }}
              </span>
            </span>
          </label>
          <label class="flex cursor-pointer items-center gap-2 text-sm">
            <ShadcnRadioGroupItem
              :value="OTHER"
              data-testid="chat-choice-other"
            />
            {{ t('chat.choice.other') }}
          </label>
        </ShadcnRadioGroup>
        <UiInput
          v-if="selected === OTHER"
          v-model="otherText"
          :label="t('chat.choice.otherPlaceholder')"
          :labels="fieldLabels.input.value"
          data-testid="chat-choice-other-text"
          @keydown.enter.prevent="confirm"
        />
      </div>
    </template>
    <template #footer>
      <div class="flex justify-end gap-2">
        <UiButton size="sm" variant="outline" @click="cancel">
          {{ t('chat.choice.cancel') }}
        </UiButton>
        <UiButton
          size="sm"
          :disabled="!canConfirm"
          data-testid="chat-choice-confirm"
          @click="confirm"
        >
          {{ t('chat.choice.confirm') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
