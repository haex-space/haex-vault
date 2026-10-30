// Which of the catalog's states the app shows (spec 069, contracts/ui.md "Catalog"): pure, so the
// order of the rules is tested without a window.
//
// The shapes of the generated bindings (`CatalogPage`, `MarketplaceView`) as far as the rules use
// them, declared here so the check scripts load this file without the bindings.
type MarketplaceView = { enabled: boolean }
type CatalogPage = {
  entries: readonly unknown[]
  sourceErrors: readonly unknown[]
}

export type CatalogState =
  /** Nothing has been answered yet. */
  | 'loading'
  /** No marketplace is in the list at all. */
  | 'none'
  /** Marketplaces exist but every one is switched off. */
  | 'allOff'
  /** Every marketplace that was asked failed. */
  | 'offline'
  /** Answered, but nothing matches. */
  | 'empty'
  | 'ready'

export function catalogState(input: {
  markets: readonly MarketplaceView[] | null
  page: CatalogPage | null
}): CatalogState {
  const { markets, page } = input
  if (markets === null) return 'loading'
  if (markets.length === 0) return 'none'
  const enabled = markets.filter((market) => market.enabled)
  if (enabled.length === 0) return 'allOff'
  if (page === null) return 'loading'
  if (page.entries.length > 0) return 'ready'
  // Every asked source failed: the answer holds an error line for each and no entry.
  if (page.sourceErrors.length >= enabled.length) return 'offline'
  return 'empty'
}

/** The query of the route as the host's `CatalogQuery` wants it: an empty value is no filter. */
export function catalogQueryOf(query: Record<string, string>): {
  page: number
  search?: string
  category?: string
  sourceId?: string
} {
  const page = Number.parseInt(query.page ?? '', 10)
  const clean = (value: string | undefined) => {
    const trimmed = value?.trim()
    return trimmed ? trimmed : undefined
  }
  return {
    page: Number.isInteger(page) && page >= 1 ? page : 1,
    search: clean(query.q),
    category: clean(query.category),
    sourceId: clean(query.source),
  }
}
