import type {
  MarketplaceExtensionViewModel,
} from '~/types/haexspace'
import type { AggregatedExtension } from '@/composables/useMarketplaces'
import { useMarketplaces } from '@/composables/useMarketplaces'

/**
 * Catalog facade for the marketplace UI.
 *
 * Wraps {@link useMarketplaces} and adds:
 * - reactive search/category/pagination state
 * - debounced search wiring
 * - computed category items for the select menu
 * - view-model projection that joins marketplace listings with the locally
 *   installed extensions (so cards know whether an item is installed).
 *
 * Behaviour is intentionally identical to the previous inline implementation
 * in `marketplace.vue` — this composable only relocates state, no logic
 * changes.
 */
export function useMarketplaceCatalog() {
  const { t } = useI18n()
  const { add } = useToast()
  const extensionStore = useExtensionsStore()
  const marketplace = useMarketplaces()

  // Filter state
  const searchQuery = ref('')
  const selectedCategory = ref<string | null>(null)
  const currentPage = ref(1)
  const isInitialLoading = ref(true)

  // Debounced search
  const debouncedSearch = refDebounced(searchQuery, 300)

  // Category items for select menu
  const categoryItems = computed(() => {
    const allCategory = { id: null, label: t('category.all') }
    const apiCategories = marketplace.categories.value.map((cat) => ({
      id: cat.slug,
      label: cat.name,
    }))
    return [allCategory, ...apiCategories]
  })

  // Transform API extensions to view models with installation status
  const extensionViewModels = computed((): MarketplaceExtensionViewModel[] => {
    return (marketplace.extensions.value as AggregatedExtension[]).map((ext) => {
      const installedExt = extensionStore.availableExtensions.find(
        (installed) => installed.name === ext.name,
      )
      return {
        ...ext,
        isInstalled: !!installedExt,
        installedVersion: installedExt?.version,
        latestVersion: ext.versions?.[0]?.version,
        sourceMarketplaceId: ext.sourceMarketplaceId,
        sourceMarketplaceName: ext.sourceMarketplaceName,
      }
    })
  })

  // Load extensions from API
  const loadExtensionsAsync = async () => {
    try {
      await marketplace.fetchExtensions({
        page: currentPage.value,
        limit: 20,
        category: selectedCategory.value || undefined,
        search: debouncedSearch.value || undefined,
        sort: 'downloads',
      })
    } catch (error) {
      console.error('Failed to load marketplace extensions:', error)
      add({ color: 'error', description: t('error.loadExtensions') })
    }
  }

  // Load categories from API
  const loadCategoriesAsync = async () => {
    try {
      await marketplace.fetchCategories()
    } catch (error) {
      console.error('Failed to load categories:', error)
    }
  }

  // Reset page on filter changes; if already on page 1, load immediately.
  watch([debouncedSearch, selectedCategory], () => {
    if (currentPage.value !== 1) {
      currentPage.value = 1
      return
    }
    loadExtensionsAsync()
  })

  // Page navigation always triggers a load.
  watch(currentPage, () => {
    loadExtensionsAsync()
  })

  return {
    // Underlying marketplace facade (for sourceErrors, isLoading, extensionsTotal, getDownloadUrl)
    marketplace,
    // Filter state
    searchQuery,
    selectedCategory,
    currentPage,
    isInitialLoading,
    debouncedSearch,
    // Derived
    categoryItems,
    extensionViewModels,
    // Actions
    loadExtensionsAsync,
    loadCategoriesAsync,
  }
}
