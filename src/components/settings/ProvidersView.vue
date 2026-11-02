<script setup lang="ts">
/**
 * "Anbieter": the subscriptions (Claude, Codex) and the OpenAI-compatible endpoints (OpenRouter,
 * a GPU machine, a school server) as chat backends, on one page. Both parts keep their own
 * component and their own data; this view only stacks them under a heading each.
 */
const { t } = useI18n()
</script>

<template>
  <div class="flex flex-col gap-6" data-testid="settings-providers">
    <section class="flex flex-col gap-3">
      <h2 class="px-1 text-sm font-medium text-muted-foreground">
        {{ t('settings.providers.subscriptions') }}
      </h2>
      <SettingsConnectDelegateProvider />
    </section>
    <section class="flex flex-col gap-3">
      <h2 class="px-1 text-sm font-medium text-muted-foreground">
        {{ t('settings.providers.endpoints') }}
      </h2>
      <SettingsModelServersView />
    </section>
  </div>
</template>
