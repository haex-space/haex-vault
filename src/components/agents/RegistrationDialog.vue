<script setup lang="ts">
/**
 * May an external agent use holzi, and with which rights? (spec 021, US1, FR-006, FR-009,
 * contracts/tauri-commands.md). Shows what the agent reports about itself, marked as its own claim,
 * and lets the user choose per scope of "holzi bedienen" nothing, reading, or reading and changing,
 * plus the approval mode. Preset: reading everywhere, Manuell. Closing the dialog declines once,
 * without remembering. Rust checks the rights again; rights it refuses count as declining.
 */
import type { AgentMode } from '~/types/bindings/AgentMode'
import type { OperateLevel } from '~/types/bindings/OperateLevel'
import type { RegistrationQuestion } from '~/types/bindings/RegistrationQuestion'
import type { Rights } from '~/types/bindings/Rights'

const { t } = useI18n()
const { errString } = useErrorString()
const store = useMcpAgentsStore()

/** The scopes of "holzi bedienen" (Rust `grants::OPERATE_SCOPES`), in the order they are shown. */
const OPERATE_SCOPES = [
  'wm.read',
  'wm.navigation',
  'wm.layout',
  'chat.read',
  'chat.write',
  'settings.read',
  'settings.device',
  'settings.models',
] as const

type Level = OperateLevel | 'none'
type Scope = (typeof OPERATE_SCOPES)[number]

const question = computed<RegistrationQuestion | null>(
  () => store.shown?.question ?? null,
)
const levels = ref<Record<Scope, Level>>(
  Object.fromEntries(OPERATE_SCOPES.map((scope) => [scope, 'none'])) as Record<
    Scope,
    Level
  >,
)
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
    levels.value = Object.fromEntries(
      OPERATE_SCOPES.map((scope) => [
        scope,
        asked.rights.operate[scope] ?? 'none',
      ]),
    ) as Record<Scope, Level>
    mode.value = asked.mode
  },
  { immediate: true },
)

const levelOptions = computed(() =>
  (['none', 'read', 'write'] as const).map((value) => ({
    value,
    label: t(`mcpAgents.level.${value}`),
  })),
)

function rights(): Rights {
  const operate: Record<string, OperateLevel> = {}
  for (const [scope, level] of Object.entries(levels.value))
    if (level !== 'none') operate[scope] = level
  return { operate }
}

async function answerAsync(register: boolean) {
  const shown = store.shown
  if (!shown) return
  try {
    await store.answerAsync(shown.requestId, {
      decision: register ? 'register' : 'deny',
      remember: register ? false : remember.value,
      rights: register ? rights() : { operate: {} },
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
            <span class="text-xs text-muted-foreground">
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

        <SettingsGroup :label="t('mcpAgents.registration.rights')">
          <SettingsRow
            v-for="scope in OPERATE_SCOPES"
            :key="scope"
            :title="t(`actions.scopes.${scope}`)"
            :label-for="`mcp-right-${scope}`"
          >
            <SettingsSelect
              :id="`mcp-right-${scope}`"
              v-model="levels[scope]"
              class="w-44"
              :options="levelOptions"
              :data-testid="`mcp-right-${scope}`"
            />
          </SettingsRow>
        </SettingsGroup>
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

        <label class="flex items-center gap-2 text-sm">
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
          @click="answerAsync(true)"
        >
          {{ t('mcpAgents.registration.register') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
