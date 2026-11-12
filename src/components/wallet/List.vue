<script setup lang="ts">
/**
 * The list of Wallet (spec 092, Phase W): every pass outside the trash, flat, in the order of the
 * date sort (FR-021: upcoming first, then past, then undated), filtered by the search text of the
 * place (`?q=`). Logins are not part of it. While the overview has not been loaded once it shows a
 * spinner instead of the empty state.
 */
import { displayTitle } from '~/lib/passwords/format'
import { filterHeaders } from '~/lib/passwords/search'
import { entryTypeOf } from '~/lib/passwords/passType'
import { sortByDate } from '~/lib/passwords/sortFilter'
import { isInTrash, trashGroupIds } from '~/lib/passwords/tree'

const { t } = useI18n()
const router = useTabRouter()
const store = usePasswordsStore()

const searching = computed(() => Boolean(router.route.query.q))
/** The open folder (`/folder/:id`), or `null` for "All". */
const folderId = computed(() =>
  router.route.path.startsWith('/folder/')
    ? (router.route.params.id ?? null)
    : null,
)
const folder = computed(() =>
  folderId.value === null
    ? null
    : (store.walletFolders.find((group) => group.id === folderId.value) ??
      null),
)

const visible = computed(() => {
  const trash = trashGroupIds(store.groups)
  // "All" is flat across the folders; a folder shows its own passes; a search looks through all.
  const passes = store.headers.filter(
    (header) =>
      entryTypeOf(header) === 'pass' &&
      !isInTrash(header.groupId, trash) &&
      (searching.value ||
        folderId.value === null ||
        header.groupId === folderId.value),
  )
  // Sorted by date first, so hits that rank alike keep that order.
  return filterHeaders(sortByDate(passes, new Date()), {
    query: router.route.query.q ?? '',
  })
})

// ponytail: the passes mount in pages of `PAGE` as the list scrolls near its end. Ceiling: after
// scrolling to the end of a large wallet every row is mounted; a virtual list is the upgrade.
const PAGE = 100
const shownCount = ref(PAGE)
const shown = computed(() => visible.value.slice(0, shownCount.value))
watch(
  () => [router.route.path, router.route.query.q],
  () => {
    shownCount.value = PAGE
  },
)

const area = useTemplateRef<HTMLElement>('area')
useInfiniteScroll(
  area,
  () => {
    shownCount.value += PAGE
  },
  {
    distance: 800,
    canLoadMore: () => shownCount.value < visible.value.length,
  },
)

function open(id: string) {
  void router.push(`/entry/${id}`)
}
</script>

<template>
  <div
    ref="area"
    class="min-h-0 flex-1 overflow-y-auto px-4 py-3 pb-6 outline-none @md:px-6"
    tabindex="-1"
    data-testid="wallet-list-area"
  >
    <div class="mx-auto flex w-full max-w-3xl flex-col gap-3">
      <div
        v-if="!store.hasLoadedOnce"
        class="flex justify-center py-10 text-muted-foreground"
        role="status"
        data-testid="wallet-loading"
      >
        <Icon name="lucide:loader-circle" class="size-6 animate-spin" />
        <span class="sr-only">{{ t('passwords.loading') }}</span>
      </div>
      <h1
        v-if="store.hasLoadedOnce && folderId !== null && !searching"
        class="truncate px-1 pt-1 text-2xl font-bold"
        data-testid="wallet-folder-title"
      >
        {{ displayTitle(folder?.name) ?? t('wallet.folderGone') }}
      </h1>
      <p
        v-if="store.hasLoadedOnce && visible.length === 0"
        class="py-10 text-center text-muted-foreground"
        data-testid="wallet-empty"
      >
        {{
          searching
            ? t('wallet.noMatches')
            : folderId !== null
              ? t('wallet.folderEmpty')
              : t('wallet.empty')
        }}
      </p>
      <SettingsGroup v-else-if="store.hasLoadedOnce">
        <WalletListItem
          v-for="(header, index) in shown"
          :key="header.id"
          :header="header"
          :position="index + 1"
          :count="visible.length"
          @open="open(header.id)"
        />
      </SettingsGroup>
    </div>
  </div>
</template>
