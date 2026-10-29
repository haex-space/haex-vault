<script setup lang="ts">
/**
 * The questions of agents (spec 021, US1, US3; spec 065): the registration dialog, the call prompt
 * and the questions of the agent browser, mounted once in the desktop. For each question holzi comes to the front; where the system
 * keeps it in the back, a system notification leads to it (FR-023), the same way the External
 * Bridge does.
 */
import { invoke } from '@tauri-apps/api/core'
import { raiseWindowAsync } from '~/composables/useExtensionHost'

const { t } = useI18n()
const store = useMcpAgentsStore()
const browser = useAgentBrowserStore()

async function comeForwardAsync(kind: 'registration' | 'call' | 'browser') {
  await raiseWindowAsync()
  if (document.hasFocus()) return
  const texts =
    kind === 'browser'
      ? 'agentBrowser.notification'
      : kind === 'call'
        ? 'mcpAgents.callNotification'
        : 'mcpAgents.notification'
  await invoke('browser_bridge_notify', {
    title: t(`${texts}.title`),
    body: t(`${texts}.body`),
  }).catch((error: unknown) => {
    console.warn('[mcp agents] notification failed', error)
  })
}

onMounted(() => {
  store
    .startAsync((kind) => void comeForwardAsync(kind))
    .catch((error: unknown) => {
      console.error('[mcp agents] listening failed', error)
    })
  browser
    .startAsync(() => void comeForwardAsync('browser'))
    .catch((error: unknown) => {
      console.error('[agent browser] listening failed', error)
    })
})
</script>

<template>
  <AgentsRegistrationDialog />
  <AgentsCallRequestDialog v-if="!store.shown" />
  <AgentBrowserPromptDialog v-if="!store.shown && !store.shownCall" />
</template>
