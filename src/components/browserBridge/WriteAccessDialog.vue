<script setup lang="ts">
/**
 * A client approved for reading wants to write (spec 049, US3, FR-015): "Erlauben" lets it write
 * this time, with "Merken" from now on; "Ablehnen" with "Merken" is remembered too. With a tag scope
 * and no default tag yet, the user picks the tag new entries get. Closing the dialog declines once.
 */
import type { WriteQuestion } from '~/types/bindings/WriteQuestion'

const { t } = useI18n()
const { errString } = useErrorString()
const store = useBrowserBridgeStore()

const question = computed<WriteQuestion | null>(() =>
  store.shown?.question.kind === 'write' ? store.shown.question : null,
)
const remember = ref(false)
const defaultTag = ref('')
const failure = ref<string | null>(null)

watch(
  () => store.shown?.promptId,
  () => {
    remember.value = false
    failure.value = null
    defaultTag.value = question.value?.scopeTags?.[0] ?? ''
  },
  { immediate: true },
)

const canAllow = computed(
  () => !question.value?.needsDefaultTag || defaultTag.value !== '',
)

async function answerAsync(allow: boolean) {
  const shown = store.shown
  if (!shown) return
  try {
    await store.answerAsync(shown.promptId, {
      kind: 'write',
      allow,
      remember: remember.value,
      defaultTag: allow && defaultTag.value ? defaultTag.value : null,
    })
  } catch (error) {
    failure.value = errString(error)
  }
}

function onUpdateOpen(open: boolean) {
  if (!open) void answerAsync(false)
}
</script>

<template>
  <UiDrawerModal
    v-if="question"
    :open="true"
    :title="t('browserBridge.write.title')"
    @update:open="onUpdateOpen"
  >
    <template #content>
      <div class="space-y-4" data-testid="bridge-write">
        <p class="text-sm">
          {{
            t('browserBridge.write.body', {
              name:
                question.client.clientName || t('browserBridge.client.noName'),
              action: t(`browserBridge.write.methods.${question.method}`),
            })
          }}
        </p>
        <p class="font-mono text-xs break-all text-muted-foreground">
          {{ question.client.origin }}
        </p>
        <label
          v-if="question.needsDefaultTag && question.scopeTags"
          class="flex flex-col gap-1 text-sm"
          for="bridge-write-default-tag"
        >
          {{ t('browserBridge.defaultTag.label') }}
          <SettingsSelect
            id="bridge-write-default-tag"
            v-model="defaultTag"
            class="w-full"
            :options="
              question.scopeTags.map((tag) => ({ value: tag, label: tag }))
            "
            data-testid="bridge-write-default-tag"
          />
        </label>
        <label class="flex items-center gap-2 text-sm">
          <ShadcnCheckbox
            v-model="remember"
            data-testid="bridge-write-remember"
          />
          {{ t('browserBridge.write.remember') }}
        </label>
        <p v-if="failure" class="text-sm text-destructive" role="alert">
          {{ failure }}
        </p>
      </div>
    </template>
    <template #footer>
      <div class="flex justify-end gap-2">
        <UiButton
          type="button"
          size="sm"
          variant="outline"
          data-testid="bridge-write-deny"
          @click="answerAsync(false)"
        >
          {{ t('browserBridge.approval.deny') }}
        </UiButton>
        <UiButton
          type="button"
          size="sm"
          :disabled="!canAllow"
          data-testid="bridge-write-allow"
          @click="answerAsync(true)"
        >
          {{ t('browserBridge.write.allow') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
