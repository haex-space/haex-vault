<script setup lang="ts">
/**
 * Instead of the device's folders while holzi lacks "Zugriff auf alle Dateien" on Android (spec 044
 * US7, FR-039): why it needs it, and the way into the system settings.
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
    <h2 class="text-base font-medium">{{ t('files.allAccess.title') }}</h2>
    <p class="max-w-sm text-sm text-muted-foreground">
      {{ t('files.allAccess.body') }}
    </p>
    <UiButton data-testid="files-all-access-request" @click="emit('request')">
      {{ t('files.allAccess.request') }}
    </UiButton>
  </div>
</template>
