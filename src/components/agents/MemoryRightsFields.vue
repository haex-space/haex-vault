<script setup lang="ts">
/**
 * The rights group "Gedächtnis" of an agent profile (spec 097 FR-005, FR-008): what the models in the
 * profile reach of the shared memory: nothing, reading, or reading and writing. "Standard" reads and
 * writes from the factory. External agents never reach the memory, whatever this says (FR-020).
 */
import type { MemoryLevel } from '~/lib/agents/rights'

defineProps<{ modelValue: MemoryLevel; disabled?: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [MemoryLevel] }>()

const { t } = useI18n()

const options = computed(() =>
  (['off', 'read', 'readWrite'] as const).map((value) => ({
    value,
    label: t(`mcpAgents.memory.${value}`),
  })),
)

function choose(value: unknown) {
  if (value === 'off' || value === 'read' || value === 'readWrite') {
    emit('update:modelValue', value)
  }
}
</script>

<template>
  <SettingsGroup :label="t('mcpAgents.memory.title')">
    <SettingsRow
      :title="t('mcpAgents.memory.level')"
      :description="t('mcpAgents.memory.hint')"
      label-for="agent-memory-level"
    >
      <SettingsSelect
        id="agent-memory-level"
        :model-value="modelValue"
        class="w-56 max-w-full"
        :options="options"
        :disabled="disabled"
        data-testid="agent-memory-level"
        @update:model-value="choose"
      />
    </SettingsRow>
  </SettingsGroup>
</template>
