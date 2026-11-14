// What the catalogue puts into its window's header (spec 093-app-headers, US4): the row above the
// catalogue of spec 069 as a header description, and the commands its events stand for. Pure —
// relative `.ts` imports, so the Node check script (`scripts/check-marketplace-header.ts`) loads it;
// texts come in through `t`, so nothing here needs i18n. The catalogue has no sidebar.
import type { WmHeaderControl } from '../../types/bindings/WmHeaderControl.ts'
import type { WmHeaderDescription } from '../../types/bindings/WmHeaderDescription.ts'
import type { WmHeaderEvent } from '../../types/bindings/WmHeaderEvent.ts'

export type MarketplaceHeaderState = {
  query: string
  categories: readonly { slug: string; name: string; count: number }[]
  /** The category that narrows the catalogue; `''` for all. */
  category: string
  markets: readonly { id: string; name: string; enabled: boolean }[]
  /** The marketplace that narrows the catalogue; `''` for all. */
  source: string
  loading: boolean
}

export type Translate = (key: string) => string

/** What a header event asks the catalogue to do. */
export type MarketplaceHeaderCommand =
  | { kind: 'search'; text: string }
  | { kind: 'category'; slug: string }
  | { kind: 'source'; id: string }
  | { kind: 'refresh' }

// An entry id is unique in the whole description, so the two "all" entries and the ids of the data
// are told apart by a name or a prefix (a slug or a marketplace id could be `refresh`).
const ALL_CATEGORIES = 'all-categories'
const ALL_SOURCES = 'all-sources'
const CATEGORY = 'cat:'
const SOURCE = 'src:'

export function marketplaceHeader(
  state: MarketplaceHeaderState,
  t: Translate,
): WmHeaderDescription {
  const enabled = state.markets.filter((market) => market.enabled)
  const end: WmHeaderControl[] = [
    {
      type: 'menu',
      id: 'category',
      label: t('marketplace.category.label'),
      icon: 'lucide:layout-grid',
      testId: 'marketplace-category',
      active: state.category !== '',
      collapse: { order: 20 },
      items: [
        {
          type: 'item',
          id: ALL_CATEGORIES,
          label: t('marketplace.category.all'),
          checked: state.category === '',
        },
        ...state.categories.map((category) => ({
          type: 'item' as const,
          id: `${CATEGORY}${category.slug}`,
          label: `${category.name} (${category.count})`,
          checked: state.category === category.slug,
        })),
      ],
    },
  ]
  if (enabled.length > 1) {
    end.push({
      type: 'menu',
      id: 'source',
      label: t('marketplace.source.label'),
      icon: 'lucide:store',
      testId: 'marketplace-source-filter',
      active: state.source !== '',
      collapse: { order: 20 },
      items: [
        {
          type: 'item',
          id: ALL_SOURCES,
          label: t('marketplace.source.all'),
          checked: state.source === '',
        },
        ...enabled.map((market) => ({
          type: 'item' as const,
          id: `${SOURCE}${market.id}`,
          label: market.name,
          checked: state.source === market.id,
        })),
      ],
    })
  }
  end.push({
    type: 'button',
    id: 'refresh',
    label: t('marketplace.refresh'),
    icon: 'lucide:refresh-cw',
    testId: 'marketplace-refresh',
    disabled: state.loading,
    collapse: { order: 30 },
  })
  return {
    center: [
      {
        type: 'search',
        id: 'q',
        label: t('marketplace.search.label'),
        placeholder: t('marketplace.search.placeholder'),
        value: state.query,
        testId: 'marketplace-search',
        collapse: { order: 10, form: 'shrink' },
      },
    ],
    end,
  }
}

/** The command an event stands for; `null` for an event the app does not act on. */
export function marketplaceHeaderCommand(
  event: WmHeaderEvent,
): MarketplaceHeaderCommand | null {
  const value = typeof event.value === 'string' ? event.value : null
  switch (event.id) {
    case 'q':
      return event.kind === 'input'
        ? { kind: 'search', text: String(event.value ?? '') }
        : null
    case 'category':
      if (event.kind !== 'click' || value === null) return null
      if (value === ALL_CATEGORIES) return { kind: 'category', slug: '' }
      return value.startsWith(CATEGORY)
        ? { kind: 'category', slug: value.slice(CATEGORY.length) }
        : null
    case 'source':
      if (event.kind !== 'click' || value === null) return null
      if (value === ALL_SOURCES) return { kind: 'source', id: '' }
      return value.startsWith(SOURCE)
        ? { kind: 'source', id: value.slice(SOURCE.length) }
        : null
    case 'refresh':
      return event.kind === 'click' ? { kind: 'refresh' } : null
    default:
      return null
  }
}
