<script setup lang="ts">
/**
 * May an external agent run this call? (spec 021, US3, FR-022, contracts/tauri-commands.md). Shows
 * the name the agent reports, marked as its own claim, the localized action, its effect, target and
 * inputs without secrets, and offers Erlauben and Ablehnen. Prompts of several calls come one after
 * the other, the oldest first. Closing the dialog declines the call.
 */
import type { CallQuestion } from '~/types/bindings/CallQuestion'

const { t, locale } = useI18n()
const { errString } = useErrorString()
const store = useMcpAgentsStore()

const question = computed<CallQuestion | null>(
  () => store.shownCall?.question ?? null,
)
const failure = ref<string | null>(null)
const waiting = computed(() => Math.max(store.calls.length - 1, 0))

const actionName = computed(() => {
  const asked = question.value
  if (!asked) return ''
  const title = locale.value === 'de' ? asked.titles.de : asked.titles.en
  return title || asked.actionId
})

const inputs = computed(() =>
  Object.entries(question.value?.input ?? {}).map(([name, value]) => ({
    name,
    value: typeof value === 'string' ? value : JSON.stringify(value),
  })),
)

watch(
  () => store.shownCall?.requestId,
  () => {
    failure.value = null
  },
)

async function answerAsync(allow: boolean) {
  const shown = store.shownCall
  if (!shown) return
  try {
    await store.answerCallAsync(shown.requestId, allow ? 'allow' : 'deny')
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
    :title="t('mcpAgents.call.title')"
    @update:open="onUpdateOpen"
  >
    <template #content>
      <div class="space-y-4" data-testid="mcp-call">
        <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
          <dt class="text-muted-foreground">
            {{ t('mcpAgents.call.agent') }}
          </dt>
          <dd class="min-w-0 break-words" data-testid="mcp-call-agent">
            {{ question.clientName }}
            <span class="text-xs text-muted-foreground">
              ({{ t('mcpAgents.registration.claimed') }})
            </span>
          </dd>
          <dt class="text-muted-foreground">
            {{ t('mcpAgents.call.action') }}
          </dt>
          <dd class="min-w-0 break-words" data-testid="mcp-call-action">
            {{ actionName }}
          </dd>
          <dt class="text-muted-foreground">
            {{ t('mcpAgents.call.effect') }}
          </dt>
          <dd
            class="min-w-0"
            :class="{
              'font-medium text-destructive': question.effect === 'destructive',
            }"
            data-testid="mcp-call-effect"
            :data-effect="question.effect"
          >
            {{ t(`mcpAgents.effect.${question.effect}`) }}
          </dd>
          <template v-if="question.target">
            <dt class="text-muted-foreground">
              {{ t('mcpAgents.call.target') }}
            </dt>
            <dd class="min-w-0 break-words">{{ question.target }}</dd>
          </template>
        </dl>

        <SettingsGroup :label="t('mcpAgents.call.input')">
          <SettingsRow
            v-for="input in inputs"
            :key="input.name"
            :title="input.name"
            :description="input.value"
          />
          <SettingsRow
            v-if="inputs.length === 0"
            :title="t('mcpAgents.call.noInput')"
          />
        </SettingsGroup>

        <p
          v-if="question.secret"
          class="rounded-xl bg-muted px-3 py-2 text-sm"
          role="status"
        >
          {{ t('mcpAgents.call.secret') }}
        </p>
        <p v-if="waiting > 0" class="text-xs text-muted-foreground">
          {{ t('mcpAgents.call.more', { count: waiting }) }}
        </p>
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
          data-testid="mcp-call-deny"
          @click="answerAsync(false)"
        >
          {{ t('mcpAgents.call.deny') }}
        </UiButton>
        <UiButton
          type="button"
          size="sm"
          :variant="
            question.effect === 'destructive' ? 'destructive' : 'default'
          "
          data-testid="mcp-call-allow"
          @click="answerAsync(true)"
        >
          {{ t('mcpAgents.call.allow') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
