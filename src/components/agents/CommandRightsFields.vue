<script setup lang="ts">
/**
 * The rights group "Befehle ausführen" of an agent (spec 021 US5, FR-012): off by default; when
 * on, the agent sees holzi's command tool. Every command counts as destructive, so Auto asks before
 * each one and Plan refuses it. Hidden where the device runs no commands (Android, iOS), as the
 * command tool is never offered there.
 */
defineProps<{ modelValue: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [boolean] }>()

const { t } = useI18n()
const { capabilities } = useDeviceCapabilities()
// Until the table is there, the desktop is assumed (`useDeviceCapabilities`).
const available = computed(() => capabilities.value?.commandTool ?? true)
</script>

<template>
  <template v-if="available">
    <SettingsGroup :label="t('mcpAgents.commands.title')">
      <SettingsRow
        :title="t('mcpAgents.commands.enabled')"
        :description="t('mcpAgents.commands.enabledHint')"
        label-for="mcp-commands-switch"
      >
        <ShadcnSwitch
          id="mcp-commands-switch"
          :model-value="modelValue"
          data-testid="mcp-commands-switch"
          @update:model-value="emit('update:modelValue', $event === true)"
        />
      </SettingsRow>
    </SettingsGroup>
    <p v-if="modelValue" class="px-1 text-xs text-muted-foreground">
      {{ t('mcpAgents.commands.askHint') }}
    </p>
  </template>
</template>
