<script setup lang="ts">
/**
 * One registered external agent in "KI → Externer Zugang" (spec 021, US7, FR-026): the name it
 * reports as its own claim, whether it is connected, its state, its agent profile (spec 083
 * FR-007), and when it registered and last connected. A registered or expired agent offers its
 * log and revoking it (one more click confirms); a blocked one offers lifting the block. `now`
 * comes from the list, so all rows age together.
 */
import type { AgentView } from '~/types/bindings/AgentView'
import { elapsedSince } from '~/lib/sync/deviceStatus'
import { profileLabel } from '~/lib/agents/profiles'

const props = defineProps<{ agent: AgentView; now: number }>()
const emit = defineEmits<{ unblock: []; revoke: [] }>()

const { t } = useI18n()
const router = useTabRouter()

const blocked = computed(() => props.agent.status === 'blocked')
const confirmRevoke = ref(false)

function ago(at: number): string {
  const elapsed = elapsedSince(at, props.now)
  return elapsed.unit === 'justNow'
    ? t('mcpAgents.time.justNow')
    : t(`mcpAgents.time.${elapsed.unit}`, { n: elapsed.value })
}

const summary = computed(() => {
  if (blocked.value) return t('mcpAgents.agents.blocked')
  const parts = [
    t('agentProfiles.row.profile', {
      name: profileLabel(t, props.agent.profileId, props.agent.profileName),
    }),
  ]
  if (props.agent.status === 'expired')
    parts.unshift(t('mcpAgents.agents.expired'))
  return parts.join(' · ')
})

const times = computed(() => {
  const parts = [
    t('mcpAgents.agents.registered', { when: ago(props.agent.registeredAt) }),
  ]
  if (!blocked.value && props.agent.lastSeenAt !== null) {
    parts.push(
      t('mcpAgents.agents.lastSeen', { when: ago(props.agent.lastSeenAt) }),
    )
  }
  return parts.join(' · ')
})

function openLog() {
  router.push(`/ai/access/${props.agent.id}/log`)
}

function revoke() {
  confirmRevoke.value = false
  emit('revoke')
}
</script>

<template>
  <SettingsRow
    icon="lucide:bot"
    data-testid="mcp-agent"
    :data-agent-id="agent.id"
    :data-slug="agent.slug"
    :data-status="agent.status"
    :data-connected="agent.connected || undefined"
  >
    <template #title>
      <span class="flex min-w-0 flex-wrap items-center gap-x-2">
        <span class="min-w-0 break-words font-medium">
          {{ agent.clientName }}
        </span>
        <span class="text-xs text-muted-foreground">
          ({{ t('mcpAgents.registration.claimed') }})
        </span>
        <span
          v-if="agent.connected"
          class="flex items-center gap-1 text-xs text-primary"
        >
          <span class="size-2 rounded-full bg-primary" aria-hidden="true" />
          {{ t('mcpAgents.agents.connected') }}
        </span>
      </span>
    </template>
    <template #description>
      <span class="block" data-testid="mcp-agent-summary">{{ summary }}</span>
      <span class="block">{{ times }}</span>
      <span v-if="confirmRevoke" class="block">
        {{ t('mcpAgents.agents.revokeConfirm') }}
      </span>
    </template>
    <div class="flex flex-wrap items-center justify-end gap-2">
      <UiButton
        v-if="blocked"
        type="button"
        variant="outline"
        data-testid="mcp-agent-unblock"
        @click="emit('unblock')"
      >
        {{ t('mcpAgents.agents.unblock') }}
      </UiButton>
      <template v-else-if="confirmRevoke">
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
          @click="revoke"
        >
          {{ t('mcpAgents.agents.revoke') }}
        </UiButton>
      </template>
      <template v-else>
        <UiButton
          type="button"
          variant="outline"
          data-testid="mcp-agent-log"
          @click="openLog"
        >
          {{ t('mcpAgents.log.title') }}
        </UiButton>
        <UiButton
          type="button"
          variant="outline"
          data-testid="mcp-agent-revoke"
          @click="confirmRevoke = true"
        >
          {{ t('mcpAgents.agents.revoke') }}
        </UiButton>
      </template>
    </div>
  </SettingsRow>
</template>
