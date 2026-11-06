<script setup lang="ts">
/**
 * The rights group "Erweiterungen" of an agent (spec 021 US4, FR-018): one switch per installed
 * extension, off by default, so a newly installed extension stays hidden from the agent until the
 * user turns it on. "Alle Erweiterungen" at the top turns on every extension at once, a later one
 * too, and greys out the single ones. How each tool asks is still set in the extension; the stricter rule applies.
 * Rust checks every change again.
 */
const props = defineProps<{
  modelValue: string[]
  /** "Alle Erweiterungen": every extension's tools, one installed later too (spec 083). */
  all?: boolean
  disabled?: boolean
}>()
const emit = defineEmits<{
  'update:modelValue': [string[]]
  'update:all': [boolean]
}>()

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
      :title="t('mcpAgents.extensions.all')"
      :description="t('mcpAgents.extensions.allHint')"
      label-for="mcp-extension-all"
    >
      <ShadcnSwitch
        id="mcp-extension-all"
        :model-value="all === true"
        :disabled="disabled"
        data-testid="mcp-extension-all"
        @update:model-value="emit('update:all', $event === true)"
      />
    </SettingsRow>
    <SettingsRow
      v-for="extension in installed"
      :key="extension.id"
      :title="extension.title"
      :description="extension.dev ? t('mcpAgents.extensions.dev') : undefined"
      :label-for="`mcp-extension-${extension.id}`"
    >
      <ShadcnSwitch
        :id="`mcp-extension-${extension.id}`"
        :model-value="all === true || modelValue.includes(extension.id)"
        :disabled="disabled || all === true"
        data-testid="mcp-extension-switch"
        :data-extension="extension.id"
        @update:model-value="set(extension.id, $event === true)"
      />
    </SettingsRow>
    <SettingsRow
      v-if="installed.length === 0 && all !== true"
      :title="t('mcpAgents.extensions.none')"
    />
  </SettingsGroup>
  <p class="px-1 text-xs text-muted-foreground">
    {{ t('mcpAgents.extensions.hint') }}
  </p>
</template>
