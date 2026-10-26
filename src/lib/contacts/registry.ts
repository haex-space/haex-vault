// Address book registry (spec 051): the places of the app "Kontakte" as route patterns with fixed
// title keys. Pure — the Vue side attaches the components in `components/wm/appRoutes.ts`. No
// name or value of a contact goes into a path or a tab title; places carry opaque ids only, and
// `entry/new` is the editor of a new contact; `import` reads a share in.
import { matchRoute } from '../wm/routeMatch.ts'

export type ContactsLocation = {
  id: 'list' | 'group' | 'entry' | 'edit' | 'import'
  /** Route pattern relative to `/` (`''` is `/` itself). */
  pattern: string
  /** Static title in the history list and the tab; never built from a stored value. */
  titleKey: string
}

export const CONTACTS_LOCATIONS: readonly ContactsLocation[] = [
  { id: 'list', pattern: '', titleKey: 'wm.contacts.list' },
  { id: 'group', pattern: 'group/:id', titleKey: 'wm.contacts.group' },
  { id: 'edit', pattern: 'entry/:id/edit', titleKey: 'wm.contacts.edit' },
  { id: 'entry', pattern: 'entry/:id', titleKey: 'wm.contacts.entry' },
  { id: 'import', pattern: 'import', titleKey: 'wm.contacts.import' },
]

export type ContactsRoutePattern = {
  path: string
  titleKey?: string
  locationId?: ContactsLocation['id']
  children?: readonly ContactsRoutePattern[]
}

/** A root `/` with one flat child per place. */
export function contactsRoutePatterns(): ContactsRoutePattern[] {
  return [
    {
      path: '/',
      children: CONTACTS_LOCATIONS.map((location) => ({
        path: location.pattern,
        titleKey: location.titleKey,
        locationId: location.id,
      })),
    },
  ]
}

export function contactsLocationFor(
  path: string,
): { location: ContactsLocation; params: Record<string, string> } | undefined {
  const match = matchRoute(contactsRoutePatterns(), path)
  const id = match?.chain[1]?.locationId
  const location = CONTACTS_LOCATIONS.find((candidate) => candidate.id === id)
  return location && match ? { location, params: match.params } : undefined
}

/** The id of a new contact in a path. */
export const NEW_CONTACT = 'new'
