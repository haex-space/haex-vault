<script setup lang="ts">
/**
 * The rights of an external agent per topic of "holzi bedienen" (spec 021 FR-008): nothing,
 * reading, or reading and changing. One row per topic, not per scope; the level of a row stands
 * for every scope of the topic (`OPERATE_GROUPS`). The slot adds rows to the same list, for the
 * group "Passwörter", which has the same levels. Used by the registration dialog and the agent's
 * settings page.
 */
import {
  OPERATE_GROUPS,
  groupLevel,
  withGroupLevel,
  type Levels,
  type Level,
  type OperateGroup,
} from '~/lib/agents/rights'

const props = defineProps<{ modelValue: Levels; disabled?: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [Levels] }>()

const { t } = useI18n()

const levelOptions = useRightLevelOptions()

function set(group: OperateGroup, level: string) {
  emit(
    'update:modelValue',
    withGroupLevel(props.modelValue, group, level as Level),
  )
}
</script>

<template>
  <SettingsGroup :label="t('mcpAgents.registration.rights')">
    <SettingsRow
      v-for="group in OPERATE_GROUPS"
      :key="group.id"
      :title="t(`mcpAgents.rights.groups.${group.id}.title`)"
      :description="t(`mcpAgents.rights.groups.${group.id}.description`)"
      :label-for="`mcp-right-${group.id}`"
    >
      <SettingsSelect
        :id="`mcp-right-${group.id}`"
        :model-value="groupLevel(modelValue, group)"
        class="w-44"
        :options="levelOptions"
        :disabled="disabled"
        :data-testid="`mcp-right-${group.id}`"
        @update:model-value="set(group, $event)"
      />
    </SettingsRow>
    <slot />
  </SettingsGroup>
</template>
