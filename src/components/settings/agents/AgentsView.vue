<script setup lang="ts">
/**
 * "KI → Agenten" (spec 021, US1, US7, FR-001, FR-005, FR-026): the state of the MCP access for
 * external agents, the registered and blocked agents of this device, and the setup snippets for
 * Claude Code, Codex and any other MCP client: by starting holzi's helper (desktop) or over
 * loopback HTTP, where the agent signs in through the registration dialog (PR G, the only way on a
 * phone). The snippets carry the path of holzi and the vault or the HTTP address, never a
 * credential. The switch and lifting a block go through guardrail actions
 * (`settings.agents.mcp.*`), which no agent can call. Below the agents: the local agent (spec 083
 * US5), the in-app chat agent, whose subject is the model that answers. Every local model is
 * listed with its agent profile ("Standard" until it is assigned another); the models of the
 * provider APIs are in the pick list below, which opens the page of any of them.
 */
import { toast } from 'vue-sonner'
import { invoke } from '@tauri-apps/api/core'
import { profileLabel } from '~/lib/agents/profiles'
import type { AgentView } from '~/types/bindings/AgentView'
import type { McpStatus } from '~/types/bindings/McpStatus'
import type { SnippetClient } from '~/types/bindings/SnippetClient'
import type { SnippetWay } from '~/types/bindings/SnippetWay'

const CLIENTS: readonly SnippetClient[] = ['claudeCode', 'codex', 'other']
type Snippets = Partial<Record<SnippetClient, string>>

const { t } = useI18n()
const { errString } = useErrorString()
const store = useMcpAgentsStore()
const profiles = useAgentProfilesStore()
const router = useTabRouter()
const setEnabled = useActionOrThrow('settings.agents.mcp.enable')
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

const models = computed(() =>
  profiles.subjects.filter((subject) => subject.kind === 'builtin'),
)
/**
 * The local models, those in an agent profile of their own, those taken out of "Standard" and
 * those gone but still assigned.
 */
const listedModels = computed(() =>
  models.value.filter(
    (model) =>
      model.local ||
      model.profiles.length > 0 ||
      !model.inStandard ||
      model.missing,
  ),
)
const otherModels = computed(() => [
  { value: '', label: t('agentProfiles.models.pick') },
  ...models.value
    .filter((model) => !listedModels.value.includes(model))
    .map((model) => ({ value: model.subject, label: model.name })),
])

function modelPath(subject: string): string {
  return `/ai/agents/model/${encodeURIComponent(subject.slice('model:'.length))}`
}

function openModel(subject: string) {
  if (subject) router.push(modelPath(subject))
}

async function reloadAsync() {
  try {
    status.value = await invoke<McpStatus>('mcp_status')
    if (status.value.state === 'noVault' || !available.value) return
    agents.value = await invoke<AgentView[]>('mcp_agents_list')
    now.value = Date.now()
    await readyAsync()
    snippets.value = capabilities.value?.mcpHelper
      ? await snippetsOfAsync('program')
      : {}
    httpSnippets.value =
      status.value.httpPort != null ? await snippetsOfAsync('http') : {}
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

async function unblockAsync(agent: AgentView) {
  try {
    await unblock({ agentId: agent.id })
  } catch (error: unknown) {
    toast.error(errString(error))
  }
  await reloadAsync()
}

onMounted(async () => {
  await reloadAsync()
  await profiles.reloadAsync().catch((error: unknown) => {
    toast.error(errString(error))
  })
})
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
      v-if="known.length > 0"
      :label="t('mcpAgents.agents.title')"
      data-testid="mcp-agents"
    >
      <AgentsAgentRow
        v-for="agent in known"
        :key="agent.id"
        :agent="agent"
        :now="now"
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
        @unblock="unblockAsync(agent)"
      />
    </SettingsGroup>

    <SettingsGroup
      :label="t('agentProfiles.models.title')"
      data-testid="agent-models"
    >
      <SettingsRow
        v-for="model in listedModels"
        :key="model.subject"
        icon="lucide:cpu"
        :to="modelPath(model.subject)"
        :title="model.name"
        :description="
          [
            t('agentProfiles.kind.builtin'),
            t('agentProfiles.row.profile', {
              name: profileLabel(t, model.profileId, model.profileName),
            }),
            ...(model.missing ? [t('agentProfiles.model.missing')] : []),
            ...(model.available ? [] : [t('agentProfiles.model.unavailable')]),
          ].join(' · ')
        "
        data-testid="agent-model"
        :data-subject="model.subject"
      />
      <SettingsRow
        v-if="listedModels.length === 0"
        :title="t('agentProfiles.models.none')"
        :description="t('agentProfiles.models.noneHint')"
      />
      <SettingsRow
        v-if="otherModels.length > 1"
        :title="t('agentProfiles.models.pickLabel')"
        label-for="agent-model-pick"
      >
        <SettingsSelect
          id="agent-model-pick"
          :model-value="''"
          class="w-72 max-w-full"
          :options="otherModels"
          data-testid="agent-model-pick"
          @update:model-value="openModel"
        />
      </SettingsRow>
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
  </section>
</template>
