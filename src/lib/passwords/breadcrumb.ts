// Breadcrumbs above the list (spec 036, FR-010, research R10): the root "Alle Einträge", the
// ancestors and the open folder; in the trash, a tag view and the search the name of the view
// instead. Every part but the last is a target (a link and a drop target). An open entry shows
// the path to its folder in the toolbar, every part a target (FR-010a). Pure, so
// `scripts/check-passwords-breadcrumb.ts` runs it without vue.
import { trashGroupIds, type TreeGroup } from './tree.ts'

/** What the list shows: a folder (`null` is the top level) or one of the views without a path;
 * or the open entry with its folder. */
export type BreadcrumbPlace =
  | { kind: 'folder'; id: string | null }
  | { kind: 'trash' }
  | { kind: 'tag'; name: string }
  | { kind: 'search' }
  | { kind: 'entry'; groupId: string | null }

export type Crumb =
  | { kind: 'root'; target: boolean }
  | { kind: 'folder'; id: string; name: string | null; target: boolean }
  | {
      kind: 'view'
      view: 'trash' | 'tag' | 'search'
      name?: string
      target: false
    }

export function breadcrumb(
  groups: readonly TreeGroup[],
  place: BreadcrumbPlace,
): Crumb[] {
  if (place.kind === 'trash')
    return [{ kind: 'view', view: 'trash', target: false }]
  if (place.kind === 'search')
    return [{ kind: 'view', view: 'search', target: false }]
  if (place.kind === 'tag')
    return [{ kind: 'view', view: 'tag', name: place.name, target: false }]

  if (
    place.kind === 'entry' &&
    place.groupId !== null &&
    trashGroupIds(groups).has(place.groupId)
  )
    return [{ kind: 'view', view: 'trash', target: false }]

  const byId = new Map(groups.map((group) => [group.id, group]))
  const chain: { id: string; name: string | null }[] = []
  const seen = new Set<string>()
  let current: string | null = place.kind === 'entry' ? place.groupId : place.id
  while (current !== null && !seen.has(current)) {
    seen.add(current)
    const group = byId.get(current)
    chain.unshift({ id: current, name: group?.name ?? null })
    current = group?.parentId ?? null
  }
  const crumbs: Crumb[] = [
    { kind: 'root', target: true },
    ...chain.map((part) => ({
      kind: 'folder' as const,
      id: part.id,
      name: part.name,
      target: true,
    })),
  ]
  if (place.kind === 'folder') crumbs[crumbs.length - 1]!.target = false
  return crumbs
}

/** Shortens a path to at most `max` parts on the row, counting the overflow "…" as one: the root
 * stays first, the last parts stay at the end, the middle goes into `hidden`. */
export function collapseCrumbs(
  crumbs: readonly Crumb[],
  max: number,
): { visible: Crumb[]; hidden: Crumb[] } {
  if (crumbs.length <= max) return { visible: [...crumbs], hidden: [] }
  // Root and the overflow take two places; the rest goes to the end of the path (at least one).
  const tail = Math.max(1, max - 2)
  return {
    visible: [crumbs[0]!, ...crumbs.slice(crumbs.length - tail)],
    hidden: crumbs.slice(1, crumbs.length - tail),
  }
}
