<script setup lang="ts">
/**
 * Instead of the device's folders while Android withholds "Zugriff auf alle Dateien" (spec 044
 * FR-039): why holzi needs it, and a button into the system's settings. The cloud storages stay
 * reachable through the sidebar.
 */
const emit = defineEmits<{ request: [] }>()
const { t } = useI18n()
</script>

<template>
  <div
    class="flex h-full flex-col items-center justify-center gap-3 p-6 text-center"
    data-testid="files-all-access"
  >
    <Icon name="lucide:folder-lock" class="size-10 text-muted-foreground" />
    <h2 class="text-lg font-semibold">{{ t('files.allAccess.title') }}</h2>
    <p class="max-w-md text-sm text-muted-foreground">
      {{ t('files.allAccess.text') }}
    </p>
    <UiButton
      type="button"
      data-testid="files-all-access-request"
      @click="emit('request')"
    >
      {{ t('files.allAccess.button') }}
    </UiButton>
  </div>
</template>
