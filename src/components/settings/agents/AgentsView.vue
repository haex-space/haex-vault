<script setup lang="ts">
/**
 * "KI → Agenten" (spec 021, US1, FR-001, FR-005): the state of the MCP access for external agents
 * and the setup snippets for Claude Code, Codex and any other MCP client. The snippets carry the
 * path of holzi and the vault, never a credential. The switch goes through the guardrail action
 * `settings.agents.mcp.enable`, which no agent can call.
 */
import { invoke } from '@tauri-apps/api/core'
import type { McpStatus } from '~/types/bindings/McpStatus'
import type { SnippetClient } from '~/types/bindings/SnippetClient'

const CLIENTS: readonly SnippetClient[] = ['claudeCode', 'codex', 'other']

const { t } = useI18n()
const { errString } = useErrorString()
const store = useMcpAgentsStore()
const setEnabled = useActionOrThrow('settings.agents.mcp.enable')

const status = ref<McpStatus | null>(null)
const snippets = ref<Partial<Record<SnippetClient, string>>>({})
const copied = ref<SnippetClient | null>(null)
const busy = ref(false)
const failure = ref<string | null>(null)

const available = computed(() => status.value?.state !== 'notAvailable')

async function reloadAsync() {
  try {
    status.value = await invoke<McpStatus>('mcp_status')
    failure.value = null
    if (status.value.state === 'noVault' || !available.value) return
    const entries = await Promise.all(
      CLIENTS.map(
        async (client) =>
          [
            client,
            await invoke<string>('mcp_setup_snippet', { args: { client } }),
          ] as const,
      ),
    )
    snippets.value = Object.fromEntries(entries)
  } catch (error: unknown) {
    failure.value = errString(error)
  }
}

async function toggleAsync(enabled: boolean) {
  if (busy.value) return
  busy.value = true
  try {
    status.value = (await setEnabled({ enabled })) as McpStatus
  } catch (error: unknown) {
    failure.value = errString(error)
    await reloadAsync()
  } finally {
    busy.value = false
  }
}

async function copyAsync(client: SnippetClient) {
  const text = snippets.value[client]
  if (!text) return
  try {
    await navigator.clipboard.writeText(text)
    copied.value = client
  } catch {
    copied.value = null
  }
}

onMounted(reloadAsync)
watch(() => store.changes, reloadAsync)
</script>

<template>
  <section class="flex flex-col gap-3">
    <SettingsNotOnThisDevice v-if="!available" />

    <SettingsGroup v-if="status">
      <SettingsRow
        :title="t('mcpAgents.settings.enabled')"
        :description="t('mcpAgents.settings.enabledHint')"
        label-for="mcp-switch"
      >
        <ShadcnSwitch
          id="mcp-switch"
          :model-value="status.enabled"
          :disabled="busy || !available"
          data-testid="mcp-switch"
          @update:model-value="toggleAsync($event === true)"
        />
      </SettingsRow>
      <SettingsRow
        :title="t('mcpAgents.settings.state')"
        :description="t(`mcpAgents.state.${status.state}`)"
        data-testid="mcp-state"
        :data-state="status.state"
      />
    </SettingsGroup>

    <SettingsGroup
      v-if="available && Object.keys(snippets).length > 0"
      :label="t('mcpAgents.setup.title')"
    >
      <SettingsRow
        v-for="client in CLIENTS"
        :key="client"
        :title="t(`mcpAgents.setup.${client}`)"
        :description="t(`mcpAgents.setup.${client}Hint`)"
      >
        <UiButton
          type="button"
          size="sm"
          variant="outline"
          :data-testid="`mcp-copy-${client}`"
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

    <p v-if="failure" class="px-1 text-xs text-destructive" role="alert">
      {{ failure }}
    </p>
  </section>
</template>
