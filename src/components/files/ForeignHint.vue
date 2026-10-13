<script setup lang="ts">
/**
 * The hint about foreign files in an encrypted folder (spec 048 FR-048): files another program
 * put there, such as a sync program's conflict copies. holzi neither shows nor deletes them.
 */
defineProps<{ count: number }>()

const { t } = useI18n()
</script>

<template>
  <div
    class="flex shrink-0 items-center gap-2 border-b px-3 py-2 text-xs text-muted-foreground"
    data-testid="files-foreign-hint"
  >
    <Icon name="lucide:file-question" class="size-4 shrink-0" />
    <span>{{ t('files.encrypted.foreign', { count }, count) }}</span>
  </div>
</template>
