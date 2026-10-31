<script setup lang="ts">
/**
 * The rights group "Erweiterungen" of an agent (spec 021 US4, FR-018): one switch per installed
 * extension, off by default, so a newly installed extension stays hidden from the agent until the
 * user turns it on. How each tool asks is still set in the extension; the stricter rule applies.
 * Rust checks every change again.
 */
const props = defineProps<{ modelValue: string[]; disabled?: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [string[]] }>()

const { t } = useI18n()
const extensions = useExtensionsStore()

const installed = computed(() =>
  extensions.list
    .filter((extension) => extension.state === 'installed')
    .sort((a, b) => a.title.localeCompare(b.title)),
)

/** Grants of extensions that are no longer installed are dropped with the next change. */
function set(id: string, on: boolean) {
  const kept = props.modelValue.filter(
    (granted) =>
      granted !== id &&
      installed.value.some((extension) => extension.id === granted),
  )
  emit('update:modelValue', on ? [...kept, id] : kept)
}
</script>

<template>
  <SettingsGroup :label="t('mcpAgents.extensions.title')">
    <SettingsRow
      v-for="extension in installed"
      :key="extension.id"
      :title="extension.title"
      :description="extension.dev ? t('mcpAgents.extensions.dev') : undefined"
      :label-for="`mcp-extension-${extension.id}`"
    >
      <ShadcnSwitch
        :id="`mcp-extension-${extension.id}`"
        :model-value="modelValue.includes(extension.id)"
        :disabled="disabled"
        data-testid="mcp-extension-switch"
        :data-extension="extension.id"
        @update:model-value="set(extension.id, $event === true)"
      />
    </SettingsRow>
    <SettingsRow
      v-if="installed.length === 0"
      :title="t('mcpAgents.extensions.none')"
    />
  </SettingsGroup>
  <p class="px-1 text-xs text-muted-foreground">
    {{ t('mcpAgents.extensions.hint') }}
  </p>
</template>
