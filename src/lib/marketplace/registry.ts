// Marketplace registry (spec 069): the places of the app "Erweiterungen entdecken" as route
// patterns with fixed title keys. Pure — the Vue side attaches the components in
// `components/wm/appRoutes.ts`. A path carries the slug of an entry only; the marketplace it
// belongs to travels as `?source=<id>`. No text a marketplace sent goes into a path or a title.
import { matchRoute } from '../wm/routeMatch.ts'

export type MarketplaceLocation = {
  id: 'catalog' | 'details'
  /** Route pattern relative to `/` (`''` is `/` itself). */
  pattern: string
  /** Static title in the history list and the tab; never built from what a marketplace sends. */
  titleKey: string
}

export const MARKETPLACE_LOCATIONS: readonly MarketplaceLocation[] = [
  { id: 'catalog', pattern: '', titleKey: 'wm.marketplace.catalog' },
  { id: 'details', pattern: ':slug', titleKey: 'wm.marketplace.details' },
]

export type MarketplaceRoutePattern = {
  path: string
  titleKey?: string
  locationId?: MarketplaceLocation['id']
  children?: readonly MarketplaceRoutePattern[]
}

/** A root `/` with one flat child per place. */
export function marketplaceRoutePatterns(): MarketplaceRoutePattern[] {
  return [
    {
      path: '/',
      children: MARKETPLACE_LOCATIONS.map((location) => ({
        path: location.pattern,
        titleKey: location.titleKey,
        locationId: location.id,
      })),
    },
  ]
}

export function marketplaceLocationFor(
  path: string,
):
  | { location: MarketplaceLocation; params: Record<string, string> }
  | undefined {
  const match = matchRoute(marketplaceRoutePatterns(), path)
  const id = match?.chain[1]?.locationId
  const location = MARKETPLACE_LOCATIONS.find(
    (candidate) => candidate.id === id,
  )
  return location && match ? { location, params: match.params } : undefined
}

/** The location of an entry: its slug in the path, its marketplace in the query. */
export function detailsPath(sourceId: string, slug: string): string {
  return `/${encodeURIComponent(slug)}?source=${encodeURIComponent(sourceId)}`
}
