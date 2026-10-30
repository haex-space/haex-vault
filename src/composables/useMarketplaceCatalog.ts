import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { CatalogCategories } from '@bindings/CatalogCategories'
import type { CatalogCategoryCount } from '@bindings/CatalogCategoryCount'
import type { CatalogPage } from '@bindings/CatalogPage'
import type { MarketplaceView } from '@bindings/MarketplaceView'
import { catalogQueryOf, catalogState } from '~/lib/marketplace/catalogState'
import { isCancelled } from '~/lib/marketplace/errors'
import {
  checkUpdatesAsync,
  reloadKnownUpdatesAsync,
} from '~/composables/useKnownUpdates'

/** How long the search field rests before the catalog is asked (spec 069 US1-2). */
const SEARCH_DEBOUNCE_MS = 300

/**
 * The state of the catalog place (spec 069, FR-001, FR-012, FR-017): what the tab's route says
 * (`?q`, `?category`, `?source`, `?page`) becomes one `marketplace_catalog` call, and only the
 * newest answer counts. A filter change goes back to page one. The catalog reloads quietly when
 * a marketplace is added, changed or removed (on this or another device) and when the installed
 * extensions change, so the marks of the tiles follow an install, an update or a removal without
 * the user reopening the catalog.
 */
export function useMarketplaceCatalog() {
  const router = useTabRouter()
  const extensions = useExtensionsStore()
  const { errString } = useErrorString()

  const markets = ref<MarketplaceView[] | null>(null)
  const page = ref<CatalogPage | null>(null)
  const categories = ref<CatalogCategoryCount[]>([])
  const loading = ref(false)
  /** The text of a failure of the call itself (not of one source: those are in the page). */
  const failure = ref<string | null>(null)

  const query = computed(() => catalogQueryOf(router.route.query))
  const state = computed(() =>
    catalogState({
      markets: markets.value,
      page: page.value,
      sourceId: query.value.sourceId,
    }),
  )

  let sequence = 0
  async function loadAsync(quiet = false): Promise<void> {
    const mine = ++sequence
    if (!quiet) loading.value = true
    try {
      const list = await invoke<MarketplaceView[]>('marketplace_list')
      if (mine !== sequence) return
      markets.value = list
      if (!list.some((market) => market.enabled)) {
        page.value = null
        failure.value = null
        return
      }
      const result = await invoke<CatalogPage>('marketplace_catalog', {
        query: query.value,
      })
      if (mine !== sequence) return
      page.value = result
      failure.value = null
      // The page told the host what it saw of the updates; the settings list shows it.
      void reloadKnownUpdatesAsync()
    } catch (error) {
      if (mine !== sequence || isCancelled(error)) return
      failure.value = errString(error)
    } finally {
      if (mine === sequence) loading.value = false
    }
  }

  let categorySequence = 0
  async function loadCategoriesAsync(): Promise<void> {
    const mine = ++categorySequence
    try {
      const result = await invoke<CatalogCategories>('marketplace_categories', {
        sourceId: query.value.sourceId ?? null,
      })
      if (mine === categorySequence) categories.value = result.categories
    } catch {
      // The categories only narrow the catalog; the sources' own error lines say what is wrong.
      if (mine === categorySequence) categories.value = []
    }
  }

  /** "Aktualisieren": the catalog again and a new look for updates (FR-017). */
  async function refreshAsync(): Promise<void> {
    await Promise.all([loadAsync(), loadCategoriesAsync(), checkUpdatesAsync()])
  }

  watch(
    () => JSON.stringify(query.value),
    () => void loadAsync(),
    {
      immediate: true,
    },
  )
  watch(
    () => query.value.sourceId,
    () => void loadCategoriesAsync(),
    {
      immediate: true,
    },
  )
  watch(
    () =>
      extensions.list
        .map((e) => `${e.name}@${e.version ?? ''}:${e.enabled}`)
        .join('|'),
    () => void loadAsync(true),
  )
  onVaultTablesChanged(['haex_marketplaces'], async () => {
    await Promise.all([loadAsync(true), loadCategoriesAsync()])
  })

  // The search field: the text is the user's at once, the route follows after a rest.
  const searchText = ref(router.route.query.q ?? '')
  let timer: ReturnType<typeof setTimeout> | undefined
  watch(searchText, (text) => {
    clearTimeout(timer)
    if (text.trim() === (router.route.query.q ?? '')) return
    timer = setTimeout(() => {
      flushSearch()
    }, SEARCH_DEBOUNCE_MS)
  })
  watch(
    () => router.route.query.q ?? '',
    (text) => {
      if (text !== searchText.value.trim() && text !== searchText.value)
        searchText.value = text
    },
  )
  onBeforeUnmount(() => clearTimeout(timer))

  function flushSearch(): void {
    clearTimeout(timer)
    timer = undefined
    const search = searchText.value.trim()
    if (search !== (router.route.query.q ?? ''))
      router.setQuery({ q: search || null, page: null })
  }

  return {
    markets,
    page,
    categories,
    loading,
    failure,
    state,
    query,
    searchText,
    flushSearch,
    refreshAsync,
    setCategory: (slug: string) =>
      router.setQuery({ category: slug || null, page: null }),
    setSource: (id: string) =>
      router.setQuery({ source: id || null, category: null, page: null }),
    goToPage: (next: number) =>
      router.setQuery({ page: next > 1 ? String(next) : null }),
  }
}
