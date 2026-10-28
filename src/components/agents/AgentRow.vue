<script setup lang="ts">
/**
 * One external agent in "KI → Agenten" (spec 021, US7, FR-026): the name it reports as its own
 * claim, whether it is connected, its state, approval mode, a summary of its rights, and when it
 * registered and last connected. A registered or expired agent opens its page; a blocked one offers
 * lifting the block. `now` comes from the list, so all rows age together.
 */
import type { AgentView } from '~/types/bindings/AgentView'
import { elapsedSince } from '~/lib/sync/deviceStatus'
import { rightsCount } from '~/lib/agents/rights'

const props = defineProps<{ agent: AgentView; now: number }>()
const emit = defineEmits<{ unblock: [] }>()

const { t } = useI18n()

const blocked = computed(() => props.agent.status === 'blocked')

function ago(at: number): string {
  const elapsed = elapsedSince(at, props.now)
  return elapsed.unit === 'justNow'
    ? t('mcpAgents.time.justNow')
    : t(`mcpAgents.time.${elapsed.unit}`, { n: elapsed.value })
}

const summary = computed(() => {
  if (blocked.value) return t('mcpAgents.agents.blocked')
  const { read, write } = rightsCount(props.agent.rights)
  const parts = [
    t(`mcpAgents.mode.${props.agent.mode}`),
    read + write === 0
      ? t('mcpAgents.agents.noRights')
      : t('mcpAgents.agents.rights', { read, write }),
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
</script>

<template>
  <SettingsRow
    icon="lucide:bot"
    :to="blocked ? undefined : `/ai/agents/${agent.id}`"
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
    </template>
    <UiButton
      v-if="blocked"
      type="button"
      variant="outline"
      data-testid="mcp-agent-unblock"
      @click="emit('unblock')"
    >
      {{ t('mcpAgents.agents.unblock') }}
    </UiButton>
  </SettingsRow>
</template>
