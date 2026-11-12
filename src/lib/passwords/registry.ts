// Password manager registry (spec 034-password-manager, research R13): the places of the app as
// route patterns with fixed title keys. Pure — the Vue side attaches the components in
// `components/wm/appRoutes.ts`. No title, name or value of an entry ever goes into a path, a
// query or a tab title; places carry opaque ids only (FR-039, FR-040).
import type { TabLocation } from '../wm/navigation.ts'
import { matchRoute } from '../wm/routeMatch.ts'

export type PasswordsLocation = {
  id: 'list' | 'folder' | 'entry' | 'history' | 'trash' | 'generator' | 'import'
  /** Route pattern relative to `/` (`''` is `/` itself). */
  pattern: string
  /** Static title in the history list and the tab; never built from a stored value. */
  titleKey: string
}

/** Longer patterns first only matter for readability: a pattern never matches a longer path. */
export const PASSWORDS_LOCATIONS: readonly PasswordsLocation[] = [
  { id: 'list', pattern: '', titleKey: 'wm.passwords.list' },
  { id: 'folder', pattern: 'folder/:id', titleKey: 'wm.passwords.folder' },
  {
    id: 'history',
    pattern: 'entry/:id/history',
    titleKey: 'wm.passwords.history',
  },
  { id: 'entry', pattern: 'entry/:id', titleKey: 'wm.passwords.entry' },
  { id: 'trash', pattern: 'trash', titleKey: 'wm.passwords.trash' },
  { id: 'generator', pattern: 'generator', titleKey: 'wm.passwords.generator' },
  { id: 'import', pattern: 'import', titleKey: 'wm.passwords.import' },
]

/** A `RoutePattern` that also names its location, so `appRoutes.ts` can attach the view. */
export type PasswordsRoutePattern = {
  path: string
  titleKey?: string
  locationId?: PasswordsLocation['id']
  children?: readonly PasswordsRoutePattern[]
}

/** A root `/` with one flat child per place, so the list is never rendered together with a
 * detail view. */
export function passwordsRoutePatterns(): PasswordsRoutePattern[] {
  return [
    {
      path: '/',
      children: PASSWORDS_LOCATIONS.map((location) => ({
        path: location.pattern,
        titleKey: location.titleKey,
        locationId: location.id,
      })),
    },
  ]
}

export function locationFor(
  path: string,
): { location: PasswordsLocation; params: Record<string, string> } | undefined {
  const match = matchRoute(passwordsRoutePatterns(), path)
  const id = match?.chain[1]?.locationId
  const location = PASSWORDS_LOCATIONS.find((candidate) => candidate.id === id)
  return location && match ? { location, params: match.params } : undefined
}

const OPAQUE_ID = /^[A-Za-z0-9-]{1,64}$/
/** Search text is the only free text a place may carry (title, username, URL and tag names are
 * all that the search looks at); it is never a secret and stays short. */
const MAX_QUERY_LENGTH = 200

/** The tabs of an entry (spec 036, research R2): Verlauf is the place `entry/:id/history`, the other
 * two are the query `?tab=` of `entry/:id`. */
export const ENTRY_TABS = ['details', 'extra', 'history'] as const
export type EntryTab = (typeof ENTRY_TABS)[number]

/** Tabs that live in the query; `history` has a place of its own. */
const QUERY_TABS: readonly string[] = ['details', 'extra']

/** The tab a place shows: the history place is Verlauf (unless the entry is being edited, which
 * has no Verlauf), `?tab=` picks Details or Extra, anything else reads as Details. */
export function entryTab(location: TabLocation): EntryTab {
  const found = locationFor(location.path)
  if (found?.location.id === 'history') {
    return 'edit' in location.query ? 'details' : 'history'
  }
  if (found?.location.id !== 'entry') return 'details'
  return location.query.tab === 'extra' ? 'extra' : 'details'
}

/** The place of an entry on one tab, keeping the other query keys; `history` drops `edit` and `tab`
 * (the Verlauf is only for the saved entry), the others drop or set `tab`. */
export function withEntryTab(
  itemId: string,
  tab: EntryTab,
  query: Record<string, string>,
): TabLocation {
  const { tab: _tab, edit: _edit, ...rest } = query
  if (tab === 'history')
    return { path: `/entry/${itemId}/history`, query: rest }
  const kept = 'edit' in query ? { ...rest, edit: query.edit ?? '' } : rest
  return {
    path: `/entry/${itemId}`,
    query: tab === 'extra' ? { ...kept, tab } : kept,
  }
}

/** True when a location can be stored in the tab history and the saved session without leaking a
 * value: it is a known place, its path params are opaque ids, and its query holds only the search
 * text `q`, a tag id `tag`, the edit flag `edit` and, on an entry, the tab `tab`. */
export function isSecretFreeLocation(location: TabLocation): boolean {
  const found = locationFor(location.path)
  if (!found) return false
  for (const value of Object.values(found.params)) {
    if (!OPAQUE_ID.test(value)) return false
  }
  for (const [key, value] of Object.entries(location.query)) {
    if (key === 'q') {
      if (value.length > MAX_QUERY_LENGTH) return false
    } else if (key === 'tag') {
      if (!OPAQUE_ID.test(value)) return false
    } else if (key === 'edit') {
      if (value !== '' && value !== '1') return false
    } else if (key === 'tab') {
      if (found.location.id !== 'entry' || !QUERY_TABS.includes(value))
        return false
    } else {
      return false
    }
  }
  return true
}
