<script setup lang="ts">
/**
 * One external agent (spec 021, US7, FR-026, FR-027): what it reports about itself, its rights per
 * scope and its approval mode, which apply on selection without a save button (023 FR-021) and
 * reach a connected agent from its next call, its log, and revoking it. Every change runs a
 * guardrail action (`settings.agents.mcp.*`), which no agent can call.
 */
import { invoke } from '@tauri-apps/api/core'
import {
  levelsOf,
  rightsOf,
  type Levels,
  type PasswordRights,
} from '~/lib/agents/rights'
import type { AgentMode } from '~/types/bindings/AgentMode'
import type { BrowserRights } from '~/types/bindings/BrowserRights'
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
const levels = ref<Levels>(levelsOf(null))
const browser = ref<BrowserRights | null>(null)
const extensions = ref<string[]>([])
const passwords = ref<PasswordRights | null>(null)
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
    levels.value = levelsOf(agent.value.rights)
    browser.value = agent.value.rights.browser ?? null
    extensions.value = agent.value.rights.extensions ?? []
    if (pendingRights === 0 || !sameAgent) {
      // Keep an empty tag draft editable after its save revoked access. A failed save restores
      // the actual grant, and moving to another agent always resets the draft.
      const incomplete = passwords.value?.tags?.every((tag) => !tag.trim())
      if (!sameAgent || !incomplete || agent.value.rights.passwords)
        passwords.value = agent.value.rights.passwords ?? null
    }
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
  const rights = rightsOf(
    levels.value,
    browser.value,
    extensions.value,
    passwords.value,
  )
  const agentIdAtChange = agentId.value
  pendingRights += 1
  rightsSaveQueue = rightsSaveQueue.then(async () => {
    await runAsync(() => setRights({ agentId: agentIdAtChange, rights }))
    pendingRights -= 1
    if (pendingRights === 0) await reloadAsync()
  })
}

function onLevels(next: Levels) {
  levels.value = next
  saveRights()
}

/** Spec 065: the websites of the rights group "Browser", saved with the other rights. */
function onBrowser(next: BrowserRights | null) {
  browser.value = next
  saveRights()
}

/** Spec 021 US4: the extensions whose tools the agent sees, saved with the other rights. */
function onExtensions(next: string[]) {
  extensions.value = next
  saveRights()
}

/** Spec 021 US5: the group `passwords`, saved with the other rights. */
function onPasswords(next: PasswordRights | null) {
  passwords.value = next
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

      <AgentsRightsFields
        :model-value="levels"
        @update:model-value="onLevels"
      />

      <AgentsBrowserRightsFields
        :model-value="browser"
        @update:model-value="onBrowser"
      />

      <AgentsExtensionRightsFields
        :model-value="extensions"
        @update:model-value="onExtensions"
      />

      <AgentsPasswordRightsFields
        :key="agent.id"
        :model-value="passwords"
        @update:model-value="onPasswords"
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
