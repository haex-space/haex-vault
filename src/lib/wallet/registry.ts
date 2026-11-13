// Wallet registry (spec 092-wallet-passes, Phase W): the places of the app "Wallet" as route
// patterns with fixed title keys. Pure — the Vue side attaches the components in
// `components/wm/appRoutes.ts`. No title, code or value of a pass goes into a path, a query or a tab
// title; places carry opaque ids only (FR-039, FR-040 of 034 hold here as well).
import type { TabLocation } from '../wm/navigation.ts'
import { matchRoute } from '../wm/routeMatch.ts'
import { PASS_KINDS } from '../passwords/passType.ts'

export type WalletLocation = {
  id: 'list' | 'folder' | 'entry' | 'history' | 'trash'
  /** Route pattern relative to `/` (`''` is `/` itself). */
  pattern: string
  /** Static title in the history list and the tab; never built from a stored value. */
  titleKey: string
}

/** The id of a new pass in a path. */
export const NEW_PASS = 'new'

export const WALLET_LOCATIONS: readonly WalletLocation[] = [
  { id: 'list', pattern: '', titleKey: 'wm.wallet.list' },
  { id: 'folder', pattern: 'folder/:id', titleKey: 'wm.wallet.folder' },
  {
    id: 'history',
    pattern: 'entry/:id/history',
    titleKey: 'wm.wallet.history',
  },
  { id: 'entry', pattern: 'entry/:id', titleKey: 'wm.wallet.entry' },
  { id: 'trash', pattern: 'trash', titleKey: 'wm.wallet.trash' },
]

/** A `RoutePattern` that also names its location, so `appRoutes.ts` can attach the view. */
export type WalletRoutePattern = {
  path: string
  titleKey?: string
  locationId?: WalletLocation['id']
  children?: readonly WalletRoutePattern[]
}

/** A root `/` with one flat child per place, so the list is never rendered with a detail view. */
export function walletRoutePatterns(): WalletRoutePattern[] {
  return [
    {
      path: '/',
      children: WALLET_LOCATIONS.map((location) => ({
        path: location.pattern,
        titleKey: location.titleKey,
        locationId: location.id,
      })),
    },
  ]
}

export function walletLocationFor(
  path: string,
): { location: WalletLocation; params: Record<string, string> } | undefined {
  const match = matchRoute(walletRoutePatterns(), path)
  const id = match?.chain[1]?.locationId
  const location = WALLET_LOCATIONS.find((candidate) => candidate.id === id)
  return location && match ? { location, params: match.params } : undefined
}

/**
 * The place of one pass in Wallet, for a link from another app (`wm.openApp('system.wallet', …)`,
 * spec 092 W04). The id is opaque; anything else is refused, so a link can never carry a value.
 */
export function walletEntryPath(id: string): string | null {
  return OPAQUE_ID.test(id) ? `/entry/${id}` : null
}

/**
 * The place of a new pass: one that belongs to `parentId` (the "Add linked pass" button) and/or one
 * made in the Wallet folder `folderId`. Anything that is no opaque id is left out.
 */
export function walletNewPassLocation(
  parentId?: string,
  folderId?: string,
): TabLocation {
  const query: Record<string, string> = {}
  if (parentId && OPAQUE_ID.test(parentId)) query.parent = parentId
  if (folderId && OPAQUE_ID.test(folderId)) query.folder = folderId
  return { path: `/entry/${NEW_PASS}`, query }
}

const OPAQUE_ID = /^[A-Za-z0-9-]{1,64}$/
/** Search text is the only free text a place may carry; it is never a secret and stays short. */
const MAX_QUERY_LENGTH = 200

/** The tabs of a pass: Verlauf is the place `entry/:id/history`, the other two are `?tab=`. */
const QUERY_TABS: readonly string[] = ['details', 'extra']

/**
 * True when a location can be stored in the tab history and the saved session without leaking a
 * value: it is a known place, its path params are opaque ids, and its query holds only the search
 * text `q`, the kind filter `kind` (one of the fixed kinds), the edit flag `edit` and, on an entry,
 * the tab `tab`. The new pass (`entry/new`) may also name the pass it belongs to, `parent=<id>` and the folder it is made in, `folder=<id>`.
 */
export function isWalletSecretFreeLocation(location: TabLocation): boolean {
  const found = walletLocationFor(location.path)
  if (!found) return false
  for (const value of Object.values(found.params)) {
    if (!OPAQUE_ID.test(value)) return false
  }
  for (const [key, value] of Object.entries(location.query)) {
    if (key === 'q') {
      if (value.length > MAX_QUERY_LENGTH) return false
    } else if (key === 'kind') {
      const filterable =
        found.location.id === 'list' || found.location.id === 'folder'
      if (!filterable || !PASS_KINDS.some((k) => k === value)) return false
    } else if (key === 'edit') {
      if (value !== '' && value !== '1') return false
    } else if (key === 'tab') {
      if (found.location.id !== 'entry' || !QUERY_TABS.includes(value))
        return false
    } else if (key === 'parent' || key === 'folder') {
      const isNew =
        found.location.id === 'entry' && found.params.id === NEW_PASS
      if (!isNew || !OPAQUE_ID.test(value)) return false
    } else {
      return false
    }
  }
  return true
}
