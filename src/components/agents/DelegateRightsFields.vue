<script setup lang="ts">
/**
 * The rights group "Abo-Agenten" of an agent profile (spec 083 FR-030; specs 009, 032): how the own
 * tools of an Abo-Agent run when its model is in the profile, and what the permissive mode still
 * refuses. It says nothing about holzi's tools, which the other groups limit. Without the group
 * every call asks. Each choice emits the whole group; the editor saves on selection.
 */
import {
  AUTONOMY_MODES,
  DENY_CATEGORIES,
  type AutonomyMode,
  type DelegateRights,
  type DenyCategory,
} from '~/lib/agents/rights'

const props = defineProps<{
  modelValue: DelegateRights | null
  disabled?: boolean
}>()
const emit = defineEmits<{ 'update:modelValue': [DelegateRights | null] }>()

const { t } = useI18n()

const autonomy = computed<AutonomyMode>(
  () => props.modelValue?.autonomy ?? 'standard',
)
const deny = computed(() => new Set<DenyCategory>(props.modelValue?.deny ?? []))

function chooseMode(mode: AutonomyMode) {
  emit('update:modelValue', { autonomy: mode, deny: [...deny.value] })
}

function toggle(category: DenyCategory, checked: boolean) {
  const next = new Set(deny.value)
  if (checked) next.add(category)
  else next.delete(category)
  emit('update:modelValue', {
    autonomy: autonomy.value,
    deny: DENY_CATEGORIES.filter((item) => next.has(item)),
  })
}
</script>

<template>
  <SettingsGroup
    radio
    :label="t('mcpAgents.delegate.title')"
    :model-value="autonomy"
    :disabled="disabled"
    data-testid="mcp-delegate-rights"
    @update:model-value="chooseMode($event as AutonomyMode)"
  >
    <SettingsOptionRow
      v-for="mode in AUTONOMY_MODES"
      :key="mode"
      type="radio"
      :value="mode"
      :title="t(`chat.autonomy.${mode}`)"
      :description="t(`settings.autonomyMode.${mode}Description`)"
      :data-testid="`mcp-delegate-autonomy-${mode}`"
    />
  </SettingsGroup>
  <p class="px-1 text-xs text-muted-foreground">
    {{ t('mcpAgents.delegate.hint') }}
  </p>
  <SettingsGroup
    v-if="autonomy === 'gated_permissive'"
    :label="t('mcpAgents.delegate.denyTitle')"
    data-testid="mcp-delegate-deny"
  >
    <SettingsOptionRow
      v-for="category in DENY_CATEGORIES"
      :key="category"
      type="checkbox"
      :checked="deny.has(category)"
      :disabled="disabled"
      :title="t(`settings.denyRules.${category}`)"
      :description="t(`settings.denyRules.${category}Description`)"
      :data-testid="`mcp-delegate-deny-${category}`"
      @change="(checked: boolean) => toggle(category, checked)"
    />
  </SettingsGroup>
</template>
