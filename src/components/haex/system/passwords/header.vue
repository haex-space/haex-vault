<template>
  <div class="px-4 py-3 border-b border-default flex items-center gap-3">
    <UiInput
      v-model="searchInput"
      :placeholder="t('searchPlaceholder')"
      leading-icon="i-lucide-search"
      size="sm"
      clearable
      class="flex-1"
    />
    <div class="flex items-center gap-1 shrink-0">
      <UiButton
        :tooltip="t('viewList')"
        icon="i-lucide-list"
        variant="soft"
        size="sm"
      />
      <UiButton
        :tooltip="t('viewGrid')"
        icon="i-lucide-grid-2x2"
        variant="ghost"
        size="sm"
        disabled
      />
    </div>
  </div>
</template>

<script setup lang="ts">
const { searchInput } = storeToRefs(usePasswordsSearchStore())
const { t } = useI18n()
</script>

<i18n lang="yaml">
de:
  searchPlaceholder: Passwörter durchsuchen…
  viewList: Listenansicht
  viewGrid: Rasteransicht (in Kürze)
en:
  searchPlaceholder: Search passwords…
  viewList: List view
  viewGrid: Grid view (coming soon)
</i18n>
