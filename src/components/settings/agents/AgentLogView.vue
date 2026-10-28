<script setup lang="ts">
/**
 * The log of one external agent (spec 021, US7, FR-028): its calls newest first, with the target
 * and the inputs as holzi kept them (secrets as `•••`), the outcome and whether holzi asked.
 * Older entries load page by page; "Protokoll leeren" runs the guardrail action
 * `settings.agents.mcp.log.clear`. Entries older than 30 days are gone on their own.
 */
import { invoke } from '@tauri-apps/api/core'
import type { AgentLogEntry } from '~/types/bindings/AgentLogEntry'

const PAGE = 50

const { t } = useI18n()
const { errString } = useErrorString()
const router = useTabRouter()
const clear = useActionOrThrow('settings.agents.mcp.log.clear')

const agentId = computed(() => String(router.route.params.agentId ?? ''))
const entries = ref<AgentLogEntry[]>([])
const more = ref(false)
const loaded = ref(false)
const failure = ref<string | null>(null)
const confirmClear = ref(false)

async function pageAsync(
  before: number | null,
  beforeId: string | null = null,
): Promise<AgentLogEntry[]> {
  return invoke<AgentLogEntry[]>('mcp_agent_log', {
    args: { agentId: agentId.value, before, beforeId, limit: PAGE },
  })
}

async function reloadAsync() {
  failure.value = null
  try {
    const page = await pageAsync(null)
    entries.value = page
    more.value = page.length === PAGE
  } catch (error) {
    failure.value = errString(error)
  } finally {
    loaded.value = true
  }
}

async function loadMoreAsync() {
  const last = entries.value.at(-1)
  if (!last) return
  try {
    const page = await pageAsync(last.createdAt, last.id)
    entries.value = [...entries.value, ...page]
    more.value = page.length === PAGE
  } catch (error) {
    failure.value = errString(error)
  }
}

async function clearAsync() {
  confirmClear.value = false
  try {
    await clear({ agentId: agentId.value })
  } catch (error) {
    failure.value = errString(error)
  }
  await reloadAsync()
}

/** "Farbschema → dunkel": each input as name and value. */
function inputs(entry: AgentLogEntry): string {
  if (!entry.input) return ''
  try {
    const parsed = JSON.parse(entry.input) as Record<string, unknown>
    return Object.entries(parsed)
      .map(
        ([name, value]) =>
          `${name} → ${typeof value === 'string' ? value : JSON.stringify(value)}`,
      )
      .join(', ')
  } catch {
    return entry.input
  }
}

function details(entry: AgentLogEntry): string {
  const parts = [new Date(entry.createdAt).toLocaleString()]
  if (entry.effect) parts.push(t(`mcpAgents.effect.${entry.effect}`))
  if (entry.target)
    parts.push(t('mcpAgents.log.target', { target: entry.target }))
  const given = inputs(entry)
  if (given) parts.push(given)
  return parts.join(' · ')
}

onMounted(reloadAsync)
watch(agentId, reloadAsync)
</script>

<template>
  <section class="flex flex-col gap-3" data-testid="mcp-agent-log-view">
    <p
      v-if="loaded && entries.length === 0 && !failure"
      class="px-1 text-sm text-muted-foreground"
      role="status"
    >
      {{ t('mcpAgents.log.empty') }}
    </p>
    <SettingsGroup v-if="entries.length > 0">
      <SettingsRow
        v-for="entry in entries"
        :key="entry.id"
        :title="entry.tool"
        :description="details(entry)"
        data-testid="mcp-log-entry"
        :data-outcome="entry.outcome"
      >
        <span class="text-right text-xs">
          <span class="block">{{
            t(`mcpAgents.outcome.${entry.outcome}`)
          }}</span>
          <span v-if="entry.asked" class="block text-muted-foreground">
            {{ t('mcpAgents.log.asked') }}
          </span>
        </span>
      </SettingsRow>
    </SettingsGroup>
    <div v-if="more" class="flex justify-center">
      <UiButton type="button" variant="outline" @click="loadMoreAsync">
        {{ t('mcpAgents.log.more') }}
      </UiButton>
    </div>

    <p v-if="failure" class="px-1 text-sm text-destructive" role="alert">
      {{ failure }}
    </p>

    <SettingsGroup v-if="entries.length > 0">
      <div class="flex flex-wrap items-center justify-end gap-2 p-4">
        <template v-if="confirmClear">
          <span class="text-sm">{{ t('mcpAgents.log.clearConfirm') }}</span>
          <UiButton
            type="button"
            variant="outline"
            @click="confirmClear = false"
          >
            {{ t('mcpAgents.agents.cancel') }}
          </UiButton>
          <UiButton
            type="button"
            variant="destructive"
            data-testid="mcp-log-clear-confirm"
            @click="clearAsync"
          >
            {{ t('mcpAgents.log.clear') }}
          </UiButton>
        </template>
        <UiButton
          v-else
          type="button"
          variant="outline"
          data-testid="mcp-log-clear"
          @click="confirmClear = true"
        >
          {{ t('mcpAgents.log.clear') }}
        </UiButton>
      </div>
    </SettingsGroup>
  </section>
</template>
