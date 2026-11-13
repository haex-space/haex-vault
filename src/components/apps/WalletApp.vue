<script setup lang="ts">
/**
 * The Wallet frame (spec 092-wallet-passes, Phase W): a toolbar row above the content, laid out like
 * the frames of the password manager and the address book. Wallet shows the passes of the vault
 * (cards, tickets, vouchers) and nothing else; it reads the same overview as the password manager
 * (`usePasswordsStore`), so a change in either shows in both. Every place is a tab location (spec
 * 020); only ids go into the path.
 */
import { toast } from 'vue-sonner'

const store = usePasswordsStore()
// A failed (quiet) reload of the overview shows as a toast, once for every open Wallet.
watch(
  () => store.lastError,
  (message) => {
    if (message) toast.error(message, { id: 'wallet-load-failed' })
  },
)
onMounted(() => {
  void store.reloadAsync()
})
</script>

<template>
  <div
    class="@container relative flex h-full min-h-0 flex-col bg-muted/20"
    data-wallet-app
  >
    <WalletToolbar class="h-13 shrink-0 px-2" />
    <main class="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto">
      <WmRouterView />
    </main>
  </div>
</template>
