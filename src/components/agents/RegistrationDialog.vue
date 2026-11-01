<script setup lang="ts">
/**
 * May an external agent use holzi, and with which rights? (spec 021, US1, FR-006, FR-009,
 * contracts/tauri-commands.md). Shows what the agent reports about itself, marked as its own claim,
 * and lets the user choose per scope of "holzi bedienen" nothing, reading, or reading and changing,
 * the websites of the rights group "Browser" (spec 065), the extensions whose tools it sees (US4,
 * all off), the groups "Passwörter" and "Befehle ausführen" (US5, off), plus the approval mode.
 * Preset: reading everywhere, Manuell. Closing the dialog declines once, without remembering.
 * For a chat delegate (US6, `question.delegate`) holzi names it itself, so the name is no claim,
 * and declining is never remembered: "Rechte festlegen" asks again.
 * Rust checks the rights again; rights it refuses count as declining.
 */
import {
  passwordsComplete,
  rightsDraftOf,
  rightsOf,
  type RightsDraft,
} from '~/lib/agents/rights'
import type { AgentMode } from '~/types/bindings/AgentMode'
import type { RegistrationQuestion } from '~/types/bindings/RegistrationQuestion'

const { t } = useI18n()
const { errString } = useErrorString()
const store = useMcpAgentsStore()

const question = computed<RegistrationQuestion | null>(
  () => store.shown?.question ?? null,
)
const draft = ref<RightsDraft>(rightsDraftOf(null))
const complete = computed(() => passwordsComplete(draft.value.passwords))
const mode = ref<AgentMode>('manual')
const remember = ref(false)
const failure = ref<string | null>(null)

watch(
  () => store.shown?.requestId,
  () => {
    const asked = question.value
    failure.value = null
    remember.value = false
    if (!asked) return
    draft.value = rightsDraftOf(asked.rights)
    mode.value = asked.mode
  },
  { immediate: true },
)

async function answerAsync(register: boolean) {
  const shown = store.shown
  const asked = question.value
  if (!shown || !asked || (register && !complete.value)) return
  try {
    await store.answerAsync(shown.requestId, {
      decision: register ? 'register' : 'deny',
      remember: register || asked.delegate ? false : remember.value,
      rights: register ? rightsOf(draft.value) : { operate: {} },
      mode: mode.value,
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
    :title="t('mcpAgents.registration.title')"
    @update:open="onUpdateOpen"
  >
    <template #content>
      <div class="space-y-4" data-testid="mcp-registration">
        <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
          <dt class="text-muted-foreground">
            {{ t('mcpAgents.registration.name') }}
          </dt>
          <dd class="min-w-0 break-words" data-testid="mcp-registration-name">
            {{ question.clientName }}
            <span
              v-if="!question.delegate"
              class="text-xs text-muted-foreground"
            >
              ({{ t('mcpAgents.registration.claimed') }})
            </span>
          </dd>
          <template v-if="question.clientVersion">
            <dt class="text-muted-foreground">
              {{ t('mcpAgents.registration.version') }}
            </dt>
            <dd class="min-w-0 break-words">{{ question.clientVersion }}</dd>
          </template>
          <dt class="text-muted-foreground">
            {{ t('mcpAgents.registration.connection') }}
          </dt>
          <dd class="min-w-0">
            {{ t(`mcpAgents.transport.${question.transport}`) }}
          </dd>
        </dl>

        <AgentsRightsEditor
          v-model="draft"
          :reset-key="store.shown?.requestId"
        />
        <p class="text-xs text-muted-foreground">
          {{ t('mcpAgents.registration.filesHint') }}
        </p>
        <p class="text-xs text-muted-foreground">
          {{ t('mcpAgents.registration.rightsHint') }}
        </p>

        <SettingsGroup v-model="mode" radio :label="t('mcpAgents.mode.label')">
          <SettingsOptionRow
            v-for="value in ['manual', 'auto', 'plan'] as const"
            :key="value"
            type="radio"
            :value="value"
            :title="t(`mcpAgents.mode.${value}`)"
            :description="t(`mcpAgents.mode.${value}Hint`)"
            :data-testid="`mcp-mode-${value}`"
          />
        </SettingsGroup>

        <p
          v-if="question.delegate"
          class="text-xs text-muted-foreground"
          data-testid="mcp-registration-delegate"
        >
          {{ t('mcpAgents.registration.delegateHint') }}
        </p>
        <label v-else class="flex items-center gap-2 text-sm">
          <ShadcnCheckbox
            v-model="remember"
            data-testid="mcp-registration-remember"
          />
          {{ t('mcpAgents.registration.remember') }}
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
          data-testid="mcp-registration-deny"
          @click="answerAsync(false)"
        >
          {{ t('mcpAgents.registration.deny') }}
        </UiButton>
        <UiButton
          type="button"
          size="sm"
          data-testid="mcp-registration-register"
          :disabled="!complete"
          @click="answerAsync(true)"
        >
          {{ t('mcpAgents.registration.register') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
