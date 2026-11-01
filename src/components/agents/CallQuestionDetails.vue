<script setup lang="ts">
/**
 * What an agent's call does (spec 021, US3, FR-022): the agent's name, marked as its own claim
 * unless holzi named it, the localized action, its effect, target and inputs without secrets.
 * Shared by the call dialog and the chat's prompt for a delegate (US6).
 */
import type { CallQuestion } from '~/types/bindings/CallQuestion'

const props = defineProps<{
  question: CallQuestion
  /** A delegate of the chat: holzi named it, so the name is no claim. */
  delegate?: boolean
}>()

const { t, locale } = useI18n()

const actionName = computed(() => {
  const title =
    locale.value === 'de' ? props.question.titles.de : props.question.titles.en
  return title || props.question.actionId
})

const inputs = computed(() =>
  Object.entries(props.question.input ?? {}).map(([name, value]) => ({
    name,
    value: typeof value === 'string' ? value : JSON.stringify(value),
  })),
)
</script>

<template>
  <div class="space-y-4" data-testid="mcp-call">
    <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
      <dt class="text-muted-foreground">
        {{ t('mcpAgents.call.agent') }}
      </dt>
      <dd class="min-w-0 break-words" data-testid="mcp-call-agent">
        {{ question.clientName }}
        <span v-if="!delegate" class="text-xs text-muted-foreground">
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
    <slot />
  </div>
</template>
