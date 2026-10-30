<script setup lang="ts">
/**
 * The frame of the app "Erweiterungen entdecken" (spec 069): the place of the tab (catalog or
 * details) and, above all of it, what belongs to the one install that may run at a time: the
 * progress of the download and the install dialog of spec 017 with the bundle the host holds
 * (FR-015). Every place is a tab location (spec 020); only a slug and a marketplace id go into
 * the path.
 */
const { dialogOpen, pendingId, publisherFingerprint } = useMarketplaceInstall()

const source = computed(() =>
  pendingId.value
    ? {
        kind: 'marketplace' as const,
        id: pendingId.value,
        publisherFingerprint: publisherFingerprint.value,
      }
    : undefined,
)
</script>

<template>
  <div
    class="@container flex h-full min-h-0 flex-col bg-muted/20"
    data-testid="marketplace-app"
  >
    <main class="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto">
      <WmRouterView />
    </main>
    <MarketplaceInstallProgress />
    <ExtensionsInstallDialog v-model:open="dialogOpen" :source="source" />
  </div>
</template>
