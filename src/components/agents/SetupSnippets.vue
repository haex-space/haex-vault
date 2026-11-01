<script setup lang="ts">
/**
 * The setup snippets of one way to reach holzi (spec 021 FR-005, T095): one row per client with a
 * copy button. Shown only when the snippets are loaded.
 */
import type { SnippetClient } from '~/types/bindings/SnippetClient'

const props = defineProps<{
  snippets: Partial<Record<SnippetClient, string>>
  label: string
  hint?: string
  /** Part of the test ids, so both ways can be told apart. */
  way?: 'http'
}>()

const { t } = useI18n()
const copied = ref<SnippetClient | null>(null)
const clients = computed(() => Object.keys(props.snippets) as SnippetClient[])
const prefix = computed(() =>
  props.way ? `mcp-copy-${props.way}-` : 'mcp-copy-',
)

async function copyAsync(client: SnippetClient) {
  const text = props.snippets[client]
  if (!text) return
  try {
    await navigator.clipboard.writeText(text)
    copied.value = client
  } catch {
    copied.value = null
  }
}
</script>

<template>
  <SettingsGroup v-if="clients.length > 0" :label="label">
    <p
      v-if="hint"
      class="px-4 py-2 text-xs text-muted-foreground"
      :data-testid="`${prefix}hint`"
    >
      {{ hint }}
    </p>
    <SettingsRow
      v-for="client in clients"
      :key="client"
      :title="t(`mcpAgents.setup.${client}`)"
      :description="t(`mcpAgents.setup.${client}Hint`)"
    >
      <UiButton
        type="button"
        size="sm"
        variant="outline"
        :data-testid="`${prefix}${client}`"
        @click="copyAsync(client)"
      >
        {{
          copied === client
            ? t('mcpAgents.setup.copied')
            : t('mcpAgents.setup.copy')
        }}
      </UiButton>
    </SettingsRow>
  </SettingsGroup>
</template>
