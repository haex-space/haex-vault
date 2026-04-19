<template>
  <div class="bg-elevated/50 border-b border-default px-3 py-2 backdrop-blur-md">
    <div class="flex items-center gap-2">
      <UiInput
        v-model="searchInput"
        :placeholder="t('search')"
        leading-icon="i-lucide-search"
        clearable
        class="flex-1"
        @keydown="onSearchKeydown"
      />

      <!-- Create (Stage 2): folder / item -->
      <UiButton
        :tooltip="t('add')"
        icon="i-lucide-plus"
        color="primary"
        variant="solid"
        class="shrink-0"
        disabled
      />

      <!-- Sort (Stage 3): name / created / modified -->
      <UiButton
        :tooltip="t('sort')"
        icon="i-lucide-arrow-up-down"
        color="neutral"
        variant="outline"
        class="shrink-0"
        disabled
      />

      <!-- More (Stage 3): import submenu -->
      <UiButton
        :tooltip="t('more')"
        icon="i-lucide-more-vertical"
        color="neutral"
        variant="outline"
        class="shrink-0"
        disabled
      />
    </div>
  </div>
</template>

<script setup lang="ts">
const { searchInput } = storeToRefs(usePasswordsSearchStore())
const { t } = useI18n()

// Keep Ctrl+A scoped to the input — the layout-level shortcut will select all items in Stage 3.
const onSearchKeydown = (event: KeyboardEvent) => {
  if (event.key === 'a' && (event.ctrlKey || event.metaKey)) {
    event.stopPropagation()
  }
}
</script>

<i18n lang="yaml">
de:
  search: Suchen…
  add: Hinzufügen
  sort: Sortieren
  more: Mehr
en:
  search: Search…
  add: Add
  sort: Sort
  more: More
</i18n>
