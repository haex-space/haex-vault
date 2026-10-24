<script setup lang="ts">
/**
 * The questions of the External Bridge (spec 049, research R10): mounted once in the desktop. For
 * each question holzi comes to the front; where the system keeps it in the back, a system
 * notification leads to the dialog (Edge Cases, "holzi-Fenster minimiert"). A wrong vault password
 * in a passkey confirmation shows as a toast, since the dialog has closed by then (FR-033), and so
 * does a refused outdated haex-pass, which does not show the reason itself (FR-049).
 */
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'vue-sonner'
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
    .startAsync(
      () => void comeForwardAsync(),
      () => toast.error(t('browserBridge.presence.wrongPassphrase')),
      () => toast.warning(t('browserBridge.settings.outdatedClient')),
    )
    .catch((error: unknown) => {
      console.error('[browser bridge] listening failed', error)
    })
})
</script>

<template>
  <BrowserBridgeApprovalDialog />
  <BrowserBridgeWriteAccessDialog />
  <BrowserBridgePresenceDialog />
</template>
