<template>
  <div class="h-full overflow-y-auto">
    <div
      v-if="showInitialLoader"
      class="h-full flex flex-col items-center justify-center gap-3 text-muted p-6"
    >
      <UIcon
        name="i-lucide-loader-2"
        class="size-10 animate-spin text-primary"
      />
      <p class="text-sm text-center">
        {{ t('loading') }}
      </p>
    </div>
    <div
      v-else-if="isEmpty"
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
        :selected="item.id === selectedItemId || item.id === desktopFocusId"
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
      :final="deletingGroup ? groupsStore.isGroupInTrash(deletingGroup.id) : false"
    />
  </div>
</template>

<script setup lang="ts">
import type { SelectHaexPasswordsGroups } from '~/database/schemas'

const { t } = useI18n()

const passwordsStore = usePasswordsStore()
const { tagsByItemId, selectedItemId, isLoading, hasLoadedOnce } = storeToRefs(passwordsStore)
const { openItem } = usePasswordsNavigation()

// Only block the empty-state on the very first load — subsequent reloads
// (e.g. after delete) should keep the existing list visible to avoid flicker.
const showInitialLoader = computed(
  () => !hasLoadedOnce.value && isLoading.value,
)

const selection = usePasswordsSelectionStore()
const { desktopFocusId } = storeToRefs(selection)

const groupsStore = usePasswordsGroupsStore()

const { visibleFolders, visibleItems, isEmpty, isSearching } = usePasswordsVisibleList()
const { search } = storeToRefs(usePasswordsSearchStore())

// Children inject this to drive Shift-click range logic. The IDs are also
// provided from passwords/index.vue under 'passwords:visibleOrderedIds' for
// the selection toolbar — both keys are kept in sync via the shared composable.
const visibleOrderedIds = inject<Ref<string[]>>('passwords:visibleOrderedIds', ref([]))
provide('passwordsList:orderedIds', visibleOrderedIds)

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
  loading: Passwörter werden geladen…
en:
  noPasswords: No passwords yet
  noResults: No matches for "{query}"
  loading: Loading passwords…
</i18n>
