<script setup lang="ts">
/**
 * One external agent (spec 021, US7, FR-026, FR-027): what it reports about itself, its rights per
 * scope and its approval mode, which apply on selection without a save button (023 FR-021) and
 * reach a connected agent from its next call, its log, and revoking it. Every change runs a
 * guardrail action (`settings.agents.mcp.*`), which no agent can call.
 */
import { invoke } from '@tauri-apps/api/core'
import {
  passwordsComplete,
  rightsDraftOf,
  rightsOf,
  type RightsDraft,
} from '~/lib/agents/rights'
import type { AgentMode } from '~/types/bindings/AgentMode'
import type { AgentView } from '~/types/bindings/AgentView'

const { t } = useI18n()
const { errString } = useErrorString()
const router = useTabRouter()
const store = useMcpAgentsStore()
const setRights = useActionOrThrow('settings.agents.mcp.grants.set')
const setMode = useActionOrThrow('settings.agents.mcp.mode.set')
const revoke = useActionOrThrow('settings.agents.mcp.revoke')

const agentId = computed(() => String(router.route.params.agentId ?? ''))
const agent = ref<AgentView | null>(null)
const draft = ref<RightsDraft>(rightsDraftOf(null))
const loaded = ref(false)
const failure = ref<string | null>(null)
const confirmRevoke = ref(false)
let rightsSaveQueue = Promise.resolve()
let pendingRights = 0

async function reloadAsync() {
  const sameAgent = agent.value?.id === agentId.value
  try {
    agent.value = await invoke<AgentView>('mcp_agent_get', {
      args: { agentId: agentId.value },
    })
    const stored = rightsDraftOf(agent.value.rights)
    // Keep an empty tag draft editable after its save revoked access, and keep the password draft
    // while saves are queued. A failed save restores the actual grant, and moving to another agent
    // always resets the draft.
    const keepPasswords =
      sameAgent &&
      (pendingRights > 0 ||
        (!passwordsComplete(draft.value.passwords) && !stored.passwords))
    draft.value = keepPasswords
      ? { ...stored, passwords: draft.value.passwords }
      : stored
  } catch (error) {
    agent.value = null
    failure.value = errString(error)
  } finally {
    loaded.value = true
  }
}

async function runAsync(action: () => Promise<unknown>) {
  failure.value = null
  try {
    await action()
  } catch (error) {
    failure.value = errString(error)
  }
  await reloadAsync()
}

function saveRights() {
  const rights = rightsOf(draft.value)
  const agentIdAtChange = agentId.value
  pendingRights += 1
  rightsSaveQueue = rightsSaveQueue.then(async () => {
    await runAsync(() => setRights({ agentId: agentIdAtChange, rights }))
    pendingRights -= 1
    if (pendingRights === 0) await reloadAsync()
  })
}

/** Any rights group changed: saved with the others at once. */
function onDraft(next: RightsDraft) {
  draft.value = next
  saveRights()
}

function onMode(mode: AgentMode) {
  void runAsync(() => setMode({ agentId: agentId.value, mode }))
}

async function revokeAsync() {
  failure.value = null
  try {
    await revoke({ agentId: agentId.value })
    await router.push('/ai/agents')
  } catch (error) {
    failure.value = errString(error)
  }
}

onMounted(reloadAsync)
watch(agentId, reloadAsync)
watch(() => store.changes, reloadAsync)
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

      <AgentsRightsEditor
        :model-value="draft"
        :reset-key="agent.id"
        @update:model-value="onDraft"
      />

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

      <p v-if="failure" class="px-1 text-sm text-destructive" role="alert">
        {{ failure }}
      </p>

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
