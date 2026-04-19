<template>
  <div class="h-full overflow-y-auto">
    <div
      v-if="isEmpty"
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
      <HaexSystemPasswordsListFolder
        v-for="folder in visibleFolders"
        :key="folder.id"
        :group="folder"
        @edit="onEditFolder"
        @delete="onDeleteFolder"
      />
      <HaexSystemPasswordsListItem
        v-for="item in visibleItems"
        :key="item.id"
        :item="item"
        :tags="tagsByItemId[item.id] ?? []"
        :selected="item.id === selectedItemId"
        @click="openItem(item.id)"
      />
    </UiListContainer>

    <HaexSystemPasswordsDialogGroupEditor
      v-model:open="editorOpen"
      mode="edit"
      :group="editingGroup"
    />
    <HaexSystemPasswordsDialogDeleteGroup
      v-model:open="deleteOpen"
      :group="deletingGroup"
    />
  </div>
</template>

<script setup lang="ts">
import type { SelectHaexPasswordsGroups } from '~/database/schemas'

const { t } = useI18n()

const passwordsStore = usePasswordsStore()
const { itemsInSelectedGroup, tagsByItemId, selectedItemId } =
  storeToRefs(passwordsStore)
const { openItem } = usePasswordsNavigation()

const groupsStore = usePasswordsGroupsStore()
const { selectedGroupId, childrenByParent } = storeToRefs(groupsStore)

const { search, searchResults } = storeToRefs(usePasswordsSearchStore())

const isSearching = computed(() => searchResults.value !== null)

// Subfolders are only shown inside a group — not in "All Passwords" (that
// would duplicate what the sidebar tree already surfaces) and not while
// searching (search results are flat across the whole vault).
const visibleFolders = computed<SelectHaexPasswordsGroups[]>(() => {
  if (isSearching.value) return []
  if (selectedGroupId.value === null) return []
  return childrenByParent.value.get(selectedGroupId.value) ?? []
})

const visibleItems = computed(() =>
  isSearching.value ? searchResults.value ?? [] : itemsInSelectedGroup.value,
)

const isEmpty = computed(
  () => visibleFolders.value.length === 0 && visibleItems.value.length === 0,
)

const editorOpen = ref(false)
const editingGroup = ref<SelectHaexPasswordsGroups | null>(null)
const deleteOpen = ref(false)
const deletingGroup = ref<SelectHaexPasswordsGroups | null>(null)

const onEditFolder = (group: SelectHaexPasswordsGroups) => {
  editingGroup.value = group
  editorOpen.value = true
}

const onDeleteFolder = (group: SelectHaexPasswordsGroups) => {
  deletingGroup.value = group
  deleteOpen.value = true
}
</script>

<i18n lang="yaml">
de:
  noPasswords: Noch keine Passwörter vorhanden
  noResults: Keine Treffer für "{query}"
en:
  noPasswords: No passwords yet
  noResults: No matches for "{query}"
</i18n>
