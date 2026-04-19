<template>
  <div class="h-full overflow-y-auto">
    <div
      v-if="filteredItems.length === 0"
      class="h-full flex flex-col items-center justify-center gap-3 text-muted p-6"
    >
      <UIcon
        :name="isSearching ? 'i-lucide-search-x' : 'i-lucide-key-round'"
        class="size-12 opacity-40"
      />
      <p class="text-sm text-center">
        {{ isSearching ? t('noResults', { query: search }) : t('noPasswords') }}
      </p>
    </div>
    <UiListContainer
      v-else
      class="p-3"
    >
      <HaexSystemPasswordsListItem
        v-for="item in filteredItems"
        :key="item.id"
        :item="item"
        :tags="tagsByItemId[item.id] ?? []"
        :selected="item.id === selectedItemId"
        @click="openItem(item.id)"
      />
    </UiListContainer>
  </div>
</template>

<script setup lang="ts">
const { t } = useI18n()

const passwordsStore = usePasswordsStore()
const { items, tagsByItemId, selectedItemId } = storeToRefs(passwordsStore)
const { openItem } = passwordsStore

const { search, searchResults } = storeToRefs(usePasswordsSearchStore())

const filteredItems = computed(() =>
  searchResults.value !== null ? searchResults.value : items.value,
)

const isSearching = computed(() => searchResults.value !== null)
</script>

<i18n lang="yaml">
de:
  noPasswords: Noch keine Passwörter vorhanden
  noResults: Keine Treffer für "{query}"
en:
  noPasswords: No passwords yet
  noResults: No matches for "{query}"
</i18n>
