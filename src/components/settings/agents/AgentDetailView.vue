<script setup lang="ts">
/**
 * One agent (spec 021, US7, FR-026, FR-027, spec 083 US1): what it reports about itself, its agent
 * profile, the rights that profile gives it (shown here, changed on the profile's page), its
 * approval mode, its log, and revoking it. Choosing another profile applies on selection without a
 * save button (023 FR-021) and reaches a connected agent from its next call. Every change runs a
 * guardrail action (`settings.agents.*`), which no agent can call.
 */
import { toast } from 'vue-sonner'
import { invoke } from '@tauri-apps/api/core'
import { onVaultTablesChanged } from '~/composables/useVaultData'
import { kindLabelKey, profileLabel } from '~/lib/agents/profiles'
import { rightsDraftOf, type RightsDraft } from '~/lib/agents/rights'
import type { AgentMode } from '~/types/bindings/AgentMode'
import type { AgentView } from '~/types/bindings/AgentView'

const { t } = useI18n()
const { errString } = useErrorString()
const router = useTabRouter()
const store = useMcpAgentsStore()
const profiles = useAgentProfilesStore()
const setMode = useActionOrThrow('settings.agents.mcp.mode.set')
const revoke = useActionOrThrow('settings.agents.mcp.revoke')

const agentId = computed(() => String(router.route.params.agentId ?? ''))
const agent = ref<AgentView | null>(null)
const draft = ref<RightsDraft>(rightsDraftOf(null))
const loaded = ref(false)
const confirmRevoke = ref(false)
/**
 * Set from revoking on: the revocation's change event reloads an agent that is gone by then, and
 * that failure is no error worth a toast.
 */
let leaving = false

const profileName = computed(() =>
  agent.value
    ? profileLabel(t, agent.value.profileId, agent.value.profileName)
    : '',
)

async function reloadAsync() {
  try {
    agent.value = await invoke<AgentView>('mcp_agent_get', {
      args: { agentId: agentId.value },
    })
    draft.value = rightsDraftOf(agent.value.rights)
  } catch (error) {
    agent.value = null
    if (!leaving) toast.error(errString(error))
  } finally {
    loaded.value = true
  }
}

async function runAsync(action: () => Promise<unknown>) {
  try {
    await action()
  } catch (error) {
    toast.error(errString(error))
  }
  await reloadAsync()
}

function onMode(mode: AgentMode) {
  void runAsync(() => setMode({ agentId: agentId.value, mode }))
}

async function revokeAsync() {
  leaving = true
  try {
    await revoke({ agentId: agentId.value })
  } catch (error) {
    leaving = false
    toast.error(errString(error))
    return
  }
  router.push('/ai/agents')
}

onMounted(async () => {
  await Promise.all([reloadAsync(), profiles.reloadAsync().catch(() => {})])
})
onBeforeUnmount(() => {
  leaving = true
})
watch(agentId, reloadAsync)
watch(() => store.changes, reloadAsync)
onVaultTablesChanged(['preferences'], reloadAsync)
</script>

<template>
  <section class="flex flex-col gap-3" data-testid="mcp-agent-detail">
    <p
      v-if="loaded && !agent"
      class="px-1 text-sm text-muted-foreground"
      role="status"
    >
      {{ t('mcpAgents.agents.gone') }}
    </p>
    <template v-if="agent">
      <SettingsGroup>
        <SettingsRow
          :title="agent.clientName"
          :description="t('mcpAgents.registration.claimed')"
        />
        <SettingsRow
          v-if="agent.clientVersion"
          :title="t('mcpAgents.registration.version')"
          :description="agent.clientVersion"
        />
        <SettingsRow
          :title="t('mcpAgents.agents.state')"
          :description="
            agent.connected
              ? t('mcpAgents.agents.connected')
              : agent.status === 'expired'
                ? t('mcpAgents.agents.expired')
                : t('mcpAgents.agents.notConnected')
          "
          data-testid="mcp-agent-state"
          :data-connected="agent.connected || undefined"
        />
      </SettingsGroup>

      <SettingsAgentProfilesMemberships
        :subject="`agent:${agent.id}`"
        :description="t(kindLabelKey(agent.kind))"
      />

      <AgentsRightsEditor :model-value="draft" :reset-key="agent.id" disabled />
      <p class="px-1 text-xs text-muted-foreground">
        {{ t('agentProfiles.agent.rightsHint', { name: profileName }) }}
      </p>

      <!-- Spec 021 US5: files and contacts are this agent's rows of agent_file_permissions. -->
      <SettingsAgentFilesSetting :agent-id="agent.id" />
      <p class="px-1 text-xs text-muted-foreground">
        {{ t('mcpAgents.registration.rightsHint') }}
      </p>

      <SettingsGroup
        :model-value="agent.mode"
        radio
        :label="t('mcpAgents.mode.label')"
        @update:model-value="onMode($event as AgentMode)"
      >
        <SettingsOptionRow
          v-for="value in ['manual', 'auto', 'plan'] as const"
          :key="value"
          type="radio"
          :value="value"
          :title="t(`mcpAgents.mode.${value}`)"
          :description="t(`mcpAgents.mode.${value}Hint`)"
          :data-testid="`mcp-mode-${value}`"
        />
      </SettingsGroup>

      <SettingsGroup>
        <SettingsRow
          icon="lucide:scroll-text"
          :title="t('mcpAgents.log.title')"
          :description="t('mcpAgents.log.hint')"
          :to="`/ai/agents/${agent.id}/log`"
          data-testid="mcp-agent-log"
        />
      </SettingsGroup>

      <SettingsGroup>
        <div class="flex flex-wrap items-center justify-end gap-2 p-4">
          <template v-if="confirmRevoke">
            <span class="text-sm">{{
              t('mcpAgents.agents.revokeConfirm')
            }}</span>
            <UiButton
              type="button"
              variant="outline"
              @click="confirmRevoke = false"
            >
              {{ t('mcpAgents.agents.cancel') }}
            </UiButton>
            <UiButton
              type="button"
              variant="destructive"
              data-testid="mcp-agent-revoke-confirm"
              @click="revokeAsync"
            >
              {{ t('mcpAgents.agents.revoke') }}
            </UiButton>
          </template>
          <UiButton
            v-else
            type="button"
            variant="outline"
            data-testid="mcp-agent-revoke"
            @click="confirmRevoke = true"
          >
            {{ t('mcpAgents.agents.revoke') }}
          </UiButton>
        </div>
      </SettingsGroup>
    </template>
  </section>
</template>
