const expandedGroupIds = ref<Set<string>>(new Set())

export const useTreeExpanded = () => {
  const isExpanded = (groupId: string): boolean =>
    expandedGroupIds.value.has(groupId)

  const setExpanded = (groupId: string, expanded: boolean): void => {
    const next = new Set(expandedGroupIds.value)
    if (expanded) next.add(groupId)
    else next.delete(groupId)
    expandedGroupIds.value = next
  }

  const toggleExpanded = (groupId: string): void => {
    setExpanded(groupId, !expandedGroupIds.value.has(groupId))
  }

  const expandAncestors = (
    groupId: string,
    parentLookup: (id: string) => string | null | undefined,
  ): void => {
    const next = new Set(expandedGroupIds.value)
    let cursor = parentLookup(groupId)
    while (cursor) {
      next.add(cursor)
      cursor = parentLookup(cursor)
    }
    expandedGroupIds.value = next
  }

  return {
    isExpanded,
    setExpanded,
    toggleExpanded,
    expandAncestors,
  }
}
