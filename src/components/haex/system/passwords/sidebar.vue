<template>
  <div class="h-full flex flex-col">
    <div class="px-4 py-3 border-b border-default">
      <p class="text-xs font-semibold uppercase tracking-wider text-muted">
        {{ t('title') }}
      </p>
    </div>
    <div class="flex-1 overflow-y-auto px-2 py-2">
      <HaexSystemPasswordsTreePlaceholder />
    </div>
  </div>
</template>

<script setup lang="ts">
const { t } = useI18n()
</script>

<i18n lang="yaml">
de:
  title: Ordner
en:
  title: Folders
</i18n>
