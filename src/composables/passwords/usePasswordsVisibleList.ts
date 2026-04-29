import type { SelectHaexPasswordsGroups, SelectHaexPasswordsItemDetails } from '~/database/schemas'

/**
 * Computes the currently visible folders and items in the password list,
 * respecting the active group and search state. Shared between the list
 * component and the selection toolbar (for select-all).
 */
export const usePasswordsVisibleList = () => {
  const passwordsStore = usePasswordsStore()
  const { itemsInSelectedGroup } = storeToRefs(passwordsStore)
  const groupsStore = usePasswordsGroupsStore()
  const { selectedGroupId, childrenByParent } = storeToRefs(groupsStore)
  const { searchResults } = storeToRefs(usePasswordsSearchStore())

  const isSearching = computed(() => searchResults.value !== null)

  const visibleFolders = computed<SelectHaexPasswordsGroups[]>(() => {
    if (isSearching.value) return []
    if (selectedGroupId.value === null) return []
    return childrenByParent.value.get(selectedGroupId.value) ?? []
  })

  const visibleItems = computed<SelectHaexPasswordsItemDetails[]>(() =>
    isSearching.value ? (searchResults.value ?? []) : itemsInSelectedGroup.value,
  )

  const isEmpty = computed(
    () => visibleFolders.value.length === 0 && visibleItems.value.length === 0,
  )

  // Folders render first, matching the visual order. Used for range-selection
  // and select-all.
  const visibleOrderedIds = computed<string[]>(() => [
    ...visibleFolders.value.map((f) => f.id),
    ...visibleItems.value.map((i) => i.id),
  ])

  return { visibleFolders, visibleItems, isEmpty, isSearching, visibleOrderedIds }
}
