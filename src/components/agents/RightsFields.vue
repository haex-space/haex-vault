<script setup lang="ts">
/**
 * The rights of an external agent per scope of "holzi bedienen" (spec 021 FR-008): nothing,
 * reading, or reading and changing. Used by the registration dialog and the agent's settings page.
 */
import { OPERATE_SCOPES, type Levels, type Level } from '~/lib/agents/rights'

const props = defineProps<{ modelValue: Levels; disabled?: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [Levels] }>()

const { t } = useI18n()

const levelOptions = computed(() =>
  (['none', 'read', 'write'] as const).map((value) => ({
    value,
    label: t(`mcpAgents.level.${value}`),
  })),
)

function set(scope: (typeof OPERATE_SCOPES)[number], level: string) {
  emit('update:modelValue', { ...props.modelValue, [scope]: level as Level })
}
</script>

<template>
  <SettingsGroup :label="t('mcpAgents.registration.rights')">
    <SettingsRow
      v-for="scope in OPERATE_SCOPES"
      :key="scope"
      :title="t(`actions.scopes.${scope}`)"
      :label-for="`mcp-right-${scope}`"
    >
      <SettingsSelect
        :id="`mcp-right-${scope}`"
        :model-value="modelValue[scope]"
        class="w-44"
        :options="levelOptions"
        :disabled="disabled"
        :data-testid="`mcp-right-${scope}`"
        @update:model-value="set(scope, $event)"
      />
    </SettingsRow>
  </SettingsGroup>
</template>
