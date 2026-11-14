<script setup lang="ts">
/**
 * The Wallet frame (spec 092-wallet-passes, Phase W): a toolbar row with the sidebar button, below
 * it the sidebar with all passes, Wallet's own folders and the trash, and the content. The frame is a
 * size container, so the sidebar follows the window's width, not the screen's: from `@2xl` it sits
 * beside the content and can be hidden, below it opens over the content (`useSidebarFrame`). Wallet
 * shows the passes of the vault (cards, tickets, vouchers) and nothing else; it reads the same
 * overview as the password manager (`usePasswordsStore`), so a change in either shows in both.
 * Every place is a tab location (spec 020); only ids go into the path.
 */
import { toast } from 'vue-sonner'

const { t } = useI18n()

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

const root = useTemplateRef<HTMLElement>('root')
const sidebar = useTemplateRef<{ $el: HTMLElement }>('sidebar')

const { wideHidden, menuOpen, visible, overlaying, toggle, close } =
  useSidebarFrame(root, () => sidebar.value?.$el)

function onEscape() {
  if (!overlaying.value) return
  close()
  root.value
    ?.querySelector<HTMLElement>('[data-testid="wallet-sidebar-toggle"]')
    ?.focus()
}
</script>

<template>
  <div
    ref="root"
    class="@container relative flex h-full min-h-0 flex-col bg-muted/20"
    data-wallet-app
  >
    <WalletToolbar
      class="h-13 shrink-0 px-2"
      :sidebar-visible="visible"
      @toggle-sidebar="toggle"
    />
    <div class="relative flex min-h-0 flex-1 overflow-hidden">
      <WmSidebar
        id="wallet-sidebar"
        ref="sidebar"
        :open="menuOpen"
        :wide-hidden="wideHidden"
        :aria-label="t('wallet.title')"
        data-testid="wallet-sidebar"
        @escape="onEscape"
      >
        <WalletSidebar @navigated="close" />
      </WmSidebar>
      <main
        class="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto"
        :inert="overlaying || undefined"
      >
        <WmRouterView />
      </main>
    </div>
  </div>
</template>
