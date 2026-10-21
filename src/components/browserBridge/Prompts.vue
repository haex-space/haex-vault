<script setup lang="ts">
/**
 * The questions of the External Bridge (spec 049, research R10): mounted once in the desktop. For
 * each question holzi comes to the front; where the system keeps it in the back, a system
 * notification leads to the dialog (Edge Cases, "holzi-Fenster minimiert").
 */
import { invoke } from '@tauri-apps/api/core'
import { raiseWindowAsync } from '~/composables/useExtensionHost'

const { t } = useI18n()
const store = useBrowserBridgeStore()

async function comeForwardAsync() {
  await raiseWindowAsync()
  if (document.hasFocus()) return
  await invoke('browser_bridge_notify', {
    title: t('browserBridge.notification.title'),
    body: t('browserBridge.notification.body'),
  }).catch((error: unknown) => {
    console.warn('[browser bridge] notification failed', error)
  })
}

onMounted(() => {
  store
    .startAsync(() => void comeForwardAsync())
    .catch((error: unknown) => {
      console.error('[browser bridge] listening failed', error)
    })
})
</script>

<template>
  <BrowserBridgeApprovalDialog />
  <BrowserBridgeWriteAccessDialog />
</template>
