<script setup lang="ts">
/**
 * "KI → Agenten und Modelle" (spec 021, US7; spec 083 US5, PR M2): everything an agent profile can
 * hold, in one list: the external agents (Claude Code, Codex, other MCP clients) registered on
 * this device and the blocked ones, each opening its page with agent profile, approval mode,
 * log and removal, and the models of the in-app chat agent grouped by provider. Every model is
 * listed with its agent profile ("Standard" until it is assigned another); the models of the
 * provider APIs are in the pick list below, which opens the page of any of them. The MCP switch
 * and the setup snippets are on "Externer Zugang". Lifting a block goes through the guardrail
 * action `settings.agents.mcp.unblock`, which no agent can call.
 */
import { toast } from 'vue-sonner'
import { invoke } from '@tauri-apps/api/core'
import { groupByProvider, modelName, profileLabel } from '~/lib/agents/profiles'
import type { AgentView } from '~/types/bindings/AgentView'
import type { McpStatus } from '~/types/bindings/McpStatus'

const { t } = useI18n()
const { errString } = useErrorString()
const store = useMcpAgentsStore()
const profiles = useAgentProfilesStore()
const router = useTabRouter()
const unblock = useActionOrThrow('settings.agents.mcp.unblock')

const status = ref<McpStatus | null>(null)
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
const grouped = computed(() =>
  groupByProvider([
    ...listedModels.value,
    ...profiles.subjects.filter((subject) => subject.kind === 'provider'),
  ]),
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

function providerPath(subject: string): string {
  return `/ai/agents/provider/${encodeURIComponent(subject.slice('provider:'.length))}`
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
  } catch (error: unknown) {
    toast.error(errString(error))
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
      <template v-for="model in grouped.rest" :key="model.subject">
        <SettingsRow
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
              ...(model.available
                ? []
                : [t('agentProfiles.model.unavailable')]),
            ].join(' · ')
          "
          data-testid="agent-model"
          :data-subject="model.subject"
        />
      </template>
      <AgentsProviderGroup
        v-for="group in grouped.groups"
        :key="group.key"
        :label="group.provider ?? t('agentProfiles.models.local')"
        :count="group.items.length"
      >
        <SettingsRow
          v-if="group.providerSubject"
          icon="lucide:shield"
          :to="providerPath(group.providerSubject.subject)"
          :title="t('agentProfiles.provider.open')"
          :description="
            t('agentProfiles.row.profile', {
              name: profileLabel(
                t,
                group.providerSubject.profileId,
                group.providerSubject.profileName,
              ),
            })
          "
          data-testid="agent-provider-open"
        />
        <template v-for="model in group.items" :key="model.subject">
          <SettingsRow
            icon="lucide:cpu"
            :to="modelPath(model.subject)"
            :title="modelName(model)"
            :description="
              [
                t('agentProfiles.kind.builtin'),
                t('agentProfiles.row.profile', {
                  name: profileLabel(t, model.profileId, model.profileName),
                }),
                ...(model.missing ? [t('agentProfiles.model.missing')] : []),
                ...(model.available
                  ? []
                  : [t('agentProfiles.model.unavailable')]),
              ].join(' · ')
            "
            data-testid="agent-model"
            :data-subject="model.subject"
          />
        </template>
      </AgentsProviderGroup>
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
  </section>
</template>
