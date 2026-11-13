<script setup lang="ts">
/**
 * The quick filter by tag beside the search (spec 034, FR-011): a button that opens the tags with
 * their counts. Choosing one shows its entries (`/?tag=`), "Alle Tags" leaves the tag view. The
 * button is marked while a tag filters the list.
 */
const { t } = useI18n()
const router = useTabRouter()
const store = usePasswordsStore()

const activeTag = computed(() => {
  const id = router.route.query.tag
  return id
    ? (store.loginTags.find((tag) => tag.ids.includes(id)) ?? null)
    : null
})

function choose(id: string | null) {
  router.push(id === null ? '/' : `/?tag=${id}`)
}
</script>

<template>
  <ShadcnDropdownMenu v-if="store.loginTags.length">
    <ShadcnDropdownMenuTrigger as-child>
      <UiButton
        :variant="activeTag ? 'secondary' : 'ghost'"
        :size="activeTag ? 'default' : 'icon'"
        class="max-w-40 shrink-0"
        :aria-label="t('passwords.tagFilter.label')"
        :tooltip="t('passwords.tagFilter.label')"
        data-testid="passwords-tag-filter"
      >
        <Icon
          name="lucide:tag"
          class="size-4 shrink-0"
          :style="activeTag?.color ? { color: activeTag.color } : undefined"
        />
        <span v-if="activeTag" class="truncate">{{ activeTag.name }}</span>
      </UiButton>
    </ShadcnDropdownMenuTrigger>
    <ShadcnDropdownMenuContent align="start" class="max-h-80 overflow-y-auto">
      <ShadcnDropdownMenuItem
        class="gap-2"
        :class="activeTag ? '' : 'font-medium'"
        data-testid="passwords-tag-filter-all"
        @select="choose(null)"
      >
        <Icon name="lucide:tags" class="size-4 shrink-0" />
        <span class="flex-1">{{ t('passwords.tagFilter.all') }}</span>
      </ShadcnDropdownMenuItem>
      <ShadcnDropdownMenuSeparator />
      <ShadcnDropdownMenuItem
        v-for="tag in store.loginTags"
        :key="tag.id"
        class="gap-2"
        :class="activeTag?.id === tag.id ? 'bg-accent font-medium' : ''"
        :data-testid="`passwords-tag-filter-${tag.id}`"
        @select="choose(tag.id)"
      >
        <Icon
          name="lucide:tag"
          class="size-4 shrink-0"
          :style="tag.color ? { color: tag.color } : undefined"
        />
        <span class="min-w-0 flex-1 truncate">{{ tag.name }}</span>
        <span class="text-xs text-muted-foreground">{{ tag.itemCount }}</span>
      </ShadcnDropdownMenuItem>
    </ShadcnDropdownMenuContent>
  </ShadcnDropdownMenu>
</template>
