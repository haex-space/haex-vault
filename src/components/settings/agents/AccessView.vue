<script setup lang="ts">
/**
 * "KI → Externer Zugang" (spec 021, US1, FR-001, FR-026; spec 083 PR M2): the switch of the MCP
 * access on this device, its state, and the setup snippets for Claude Code, Codex and any other
 * MCP client: by starting holzi's helper (desktop) or over loopback HTTP, where the agent signs in
 * through the registration dialog (PR G, the only way on a phone). The snippets carry the path of
 * holzi and the vault or the HTTP address, never a credential. The switch goes through the
 * guardrail action `settings.agents.mcp.enable`, which no agent can call. Below are the agents
 * that registered, each with its log and revoking it (`settings.agents.mcp.revoke`), and the
 * blocked ones, where lifting the block goes through `settings.agents.mcp.unblock`. Their rights
 * are those of their agent profile and are on the profile's page.
 */
import { toast } from 'vue-sonner'
import { invoke } from '@tauri-apps/api/core'
import type { AgentView } from '~/types/bindings/AgentView'
import type { McpStatus } from '~/types/bindings/McpStatus'
import type { SnippetClient } from '~/types/bindings/SnippetClient'
import type { SnippetWay } from '~/types/bindings/SnippetWay'

const CLIENTS: readonly SnippetClient[] = ['claudeCode', 'codex', 'other']
type Snippets = Partial<Record<SnippetClient, string>>

const { t } = useI18n()
const { errString } = useErrorString()
const store = useMcpAgentsStore()
const setEnabled = useActionOrThrow('settings.agents.mcp.enable')
const revoke = useActionOrThrow('settings.agents.mcp.revoke')
const unblock = useActionOrThrow('settings.agents.mcp.unblock')
const { capabilities, readyAsync } = useDeviceCapabilities()

const status = ref<McpStatus | null>(null)
const snippets = ref<Snippets>({})
const httpSnippets = ref<Snippets>({})
const busy = ref(false)
const agents = ref<AgentView[]>([])
const now = ref(Date.now())
const ticker = setInterval(() => {
  now.value = Date.now()
}, 60_000)
onBeforeUnmount(() => clearInterval(ticker))

const known = computed(() => agents.value.filter((a) => a.status !== 'blocked'))
const blocked = computed(() =>
  agents.value.filter((a) => a.status === 'blocked'),
)

const available = computed(() => status.value?.state !== 'notAvailable')

async function reloadAsync() {
  try {
    status.value = await invoke<McpStatus>('mcp_status')
    if (status.value.state === 'noVault' || !available.value) return
    await readyAsync()
    snippets.value = capabilities.value?.mcpHelper
      ? await snippetsOfAsync('program')
      : {}
    httpSnippets.value =
      status.value.httpPort != null ? await snippetsOfAsync('http') : {}
    agents.value = await invoke<AgentView[]>('mcp_agents_list')
    now.value = Date.now()
  } catch (error: unknown) {
    toast.error(errString(error))
  }
}

async function snippetsOfAsync(way: SnippetWay): Promise<Snippets> {
  const entries = await Promise.all(
    CLIENTS.map(
      async (client) =>
        [
          client,
          await invoke<string>('mcp_setup_snippet', { args: { client, way } }),
        ] as const,
    ),
  )
  return Object.fromEntries(entries)
}

async function toggleAsync(enabled: boolean) {
  if (busy.value) return
  busy.value = true
  try {
    status.value = (await setEnabled({ enabled })) as McpStatus
  } catch (error: unknown) {
    toast.error(errString(error))
    await reloadAsync()
  } finally {
    busy.value = false
  }
}

async function agentActionAsync(action: () => Promise<unknown>) {
  try {
    await action()
  } catch (error: unknown) {
    toast.error(errString(error))
  }
  await reloadAsync()
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

    <AgentsSetupSnippets
      v-if="available"
      :snippets="snippets"
      :label="t('mcpAgents.setup.title')"
    />
    <AgentsSetupSnippets
      v-if="available && status?.httpPort != null"
      :snippets="httpSnippets"
      :label="t('mcpAgents.setup.httpTitle')"
      :hint="t('mcpAgents.setup.httpHint', { port: status.httpPort })"
      way="http"
    />

    <SettingsGroup
      v-if="known.length > 0"
      :label="t('mcpAgents.agents.title')"
      data-testid="mcp-agents"
    >
      <AgentsAgentRow
        v-for="agent in known"
        :key="agent.id"
        :agent="agent"
        :now="now"
        @revoke="agentActionAsync(() => revoke({ agentId: agent.id }))"
      />
    </SettingsGroup>

    <SettingsGroup
      v-if="blocked.length > 0"
      :label="t('mcpAgents.agents.blockedTitle')"
    >
      <AgentsAgentRow
        v-for="agent in blocked"
        :key="agent.id"
        :agent="agent"
        :now="now"
        @unblock="agentActionAsync(() => unblock({ agentId: agent.id }))"
      />
    </SettingsGroup>
  </section>
</template>
