<script setup lang="ts">
/**
 * Shown instead of an extension frame that could not start or did not answer (spec 017, T046).
 * Why is a toast (`useExtensionFrame`); here stays only "Neu laden".
 */
const emit = defineEmits<{ reload: [] }>()
const { t } = useI18n()
</script>

<template>
  <div
    class="flex h-full flex-col items-center justify-center gap-3 p-6 text-center"
  >
    <Icon
      name="lucide:puzzle"
      class="h-8 w-8 text-muted-foreground"
      :aria-hidden="true"
    />
    <UiButton variant="outline" @click="emit('reload')">
      {{ t('extensions.frame.reload') }}
    </UiButton>
  </div>
</template>
