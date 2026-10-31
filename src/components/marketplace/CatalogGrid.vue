<script setup lang="ts">
/**
 * The catalog place (spec 069, FR-001, FR-003, FR-012, FR-017, FR-031): the entries of every
 * marketplace that is switched on, one page at a time. The sources keep the order of the user's
 * list; each answers within its own time, so a slow or failing one shows an error line while the
 * others' entries are there. Below `@lg` the grid has one column.
 */
const { t } = useI18n()
const { openApp } = useWmTab()
const catalog = useMarketplaceCatalog()
const {
  markets,
  page,
  categories,
  loading,
  failure,
  sourceErrors,
  state,
  query,
  searchText,
  flushSearch,
} = catalog

const SETTINGS = 'system.settings'
const openOwn = () => openApp(SETTINGS, '/extensions/marketplaces/new')
const openList = () => openApp(SETTINGS, '/extensions/marketplaces')

const sourceValue = computed(() => query.value.sourceId ?? '')
const categoryValue = computed(() => query.value.category ?? '')

/** The text of a state that shows instead of the grid. */
const stateText = computed(() => {
  switch (state.value) {
    case 'loading':
      return t('marketplace.loading')
    case 'none':
      return t('marketplace.states.none')
    case 'allOff':
      return t('marketplace.states.allOff')
    case 'offline':
      return t('marketplace.states.offline')
    case 'empty':
      return t('marketplace.states.empty')
    default:
      return ''
  }
})
const stateTestId = computed(() => {
  switch (state.value) {
    case 'offline':
      return 'marketplace-offline'
    case 'none':
      return 'marketplace-none'
    case 'allOff':
      return 'marketplace-all-off'
    case 'empty':
      return 'marketplace-empty'
    default:
      return 'marketplace-loading'
  }
})
</script>

<template>
  <div class="flex flex-col gap-3 p-3">
    <MarketplaceCatalogToolbar
      v-model:search="searchText"
      :categories="categories"
      :markets="markets ?? []"
      :category="categoryValue"
      :source="sourceValue"
      :loading="loading"
      @category="catalog.setCategory"
      @source="catalog.setSource"
      @refresh="catalog.refreshAsync"
    />

    <MarketplaceSourceErrors
      :errors="sourceErrors"
      @retry="catalog.refreshAsync"
    />

    <p
      v-if="failure"
      class="rounded-xl bg-destructive/10 px-4 py-3 text-sm text-destructive"
      role="alert"
    >
      {{ failure }}
    </p>

    <ul
      v-if="state === 'ready' && page"
      class="grid grid-cols-1 gap-3 @lg:grid-cols-2 @3xl:grid-cols-3"
    >
      <MarketplaceCatalogTile
        v-for="entry in page.entries"
        :key="`${entry.source.id}/${entry.slug}`"
        :entry="entry"
        :before-open="flushSearch"
      />
    </ul>
    <div
      v-else-if="!failure || page"
      class="flex flex-col items-center gap-3 py-10"
    >
      <p
        class="text-center text-sm text-muted-foreground"
        :data-testid="stateTestId"
        :role="state === 'loading' ? 'status' : undefined"
      >
        {{ stateText }}
      </p>
      <div v-if="state === 'none'" class="flex flex-wrap justify-center gap-2">
        <UiButton
          type="button"
          variant="outline"
          data-testid="marketplace-empty-add-own"
          @click="openOwn"
        >
          {{ t('marketplace.states.addOwn') }}
        </UiButton>
      </div>
      <UiButton
        v-else-if="state === 'allOff'"
        type="button"
        variant="outline"
        data-testid="marketplace-all-off-settings"
        @click="openList"
      >
        {{ t('marketplace.states.openSettings') }}
      </UiButton>
    </div>

    <nav
      v-if="page && page.totalPages > 1"
      class="flex items-center justify-center gap-3"
      :aria-label="
        t('marketplace.pager.page', { page: page.page, total: page.totalPages })
      "
    >
      <UiButton
        variant="outline"
        size="sm"
        :disabled="page.page <= 1"
        data-testid="marketplace-page-previous"
        @click="catalog.goToPage(page.page - 1)"
      >
        {{ t('marketplace.pager.previous') }}
      </UiButton>
      <span
        class="text-sm text-muted-foreground"
        data-testid="marketplace-page"
        >{{
          t('marketplace.pager.page', {
            page: page.page,
            total: page.totalPages,
          })
        }}</span
      >
      <UiButton
        variant="outline"
        size="sm"
        :disabled="page.page >= page.totalPages"
        data-testid="marketplace-page-next"
        @click="catalog.goToPage(page.page + 1)"
      >
        {{ t('marketplace.pager.next') }}
      </UiButton>
    </nav>
  </div>
</template>
