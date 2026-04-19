<template>
  <div class="h-full overflow-y-auto">
    <UiListContainer class="p-3">
      <HaexSystemPasswordsListItem
        v-for="item in filteredItems"
        :key="item.id"
        :item="item"
        :tags="tagsByItemId[item.id] ?? []"
        :selected="item.id === selectedItemId"
        @click="selectItem(item.id)"
      />
    </UiListContainer>
  </div>
</template>

<script setup lang="ts">
const passwordsStore = usePasswordsStore()
const { items, tagsByItemId, selectedItemId } = storeToRefs(passwordsStore)
const { selectItem } = passwordsStore

const { searchResults } = storeToRefs(usePasswordsSearchStore())

const filteredItems = computed(() =>
  searchResults.value !== null ? searchResults.value : items.value,
)
</script>
