<script setup lang="ts">
/**
 * The questions of external agents (spec 021, US1, US3): the registration dialog and the call
 * prompt, mounted once in the desktop. For each question holzi comes to the front; where the system
 * keeps it in the back, a system notification leads to it (FR-023), the same way the External
 * Bridge does.
 */
import { invoke } from '@tauri-apps/api/core'
import { raiseWindowAsync } from '~/composables/useExtensionHost'

const { t } = useI18n()
const store = useMcpAgentsStore()

async function comeForwardAsync(kind: 'registration' | 'call') {
  await raiseWindowAsync()
  if (document.hasFocus()) return
  const texts = kind === 'call' ? 'callNotification' : 'notification'
  await invoke('browser_bridge_notify', {
    title: t(`mcpAgents.${texts}.title`),
    body: t(`mcpAgents.${texts}.body`),
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
})
</script>

<template>
  <AgentsRegistrationDialog />
  <AgentsCallRequestDialog v-if="!store.shown" />
</template>
