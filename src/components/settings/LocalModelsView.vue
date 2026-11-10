<script setup lang="ts">
/**
 * "Lokale Modelle": the models on this device (load, update, delete) above the way to get more
 * (HuggingFace search, running downloads, the recommended models not installed yet). A finished
 * download moves from the lower list into the upper one.
 */
const { t } = useI18n()
</script>

<template>
  <div class="flex flex-col gap-6" data-testid="settings-local-models">
    <section class="flex flex-col gap-3">
      <h2 class="px-1 text-sm font-medium text-muted-foreground">
        {{ t('settings.localModels.onDevice') }}
      </h2>
      <SettingsInstalledModels />
    </section>
    <section class="flex flex-col gap-3">
      <h2 class="px-1 text-sm font-medium text-muted-foreground">
        {{ t('settings.localModels.download') }}
      </h2>
      <SettingsDownloadModels />
    </section>
  </div>
</template>
