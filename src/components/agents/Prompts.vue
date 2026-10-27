<script setup lang="ts">
/**
 * The registration dialogs of external agents (spec 021, US1): mounted once in the desktop. For each
 * dialog holzi comes to the front; where the system keeps it in the back, a system notification
 * leads to it (FR-023), the same way the External Bridge does.
 */
import { invoke } from '@tauri-apps/api/core'
import { raiseWindowAsync } from '~/composables/useExtensionHost'

const { t } = useI18n()
const store = useMcpAgentsStore()

async function comeForwardAsync() {
  await raiseWindowAsync()
  if (document.hasFocus()) return
  await invoke('browser_bridge_notify', {
    title: t('mcpAgents.notification.title'),
    body: t('mcpAgents.notification.body'),
  }).catch((error: unknown) => {
    console.warn('[mcp agents] notification failed', error)
  })
}

onMounted(() => {
  store
    .startAsync(() => void comeForwardAsync())
    .catch((error: unknown) => {
      console.error('[mcp agents] listening failed', error)
    })
})
</script>

<template>
  <AgentsRegistrationDialog />
</template>
