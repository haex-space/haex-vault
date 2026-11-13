// The link of a pass to the entry it belongs to (spec 092, FR-013 to FR-016, research R8): what the
// pass says about its parent ("Belongs to"), the passes an entry lists ("Linked"), and the loops
// two devices can make by linking in opposite directions. The client resolves a parent against the
// headers it has, so a change by sync shows at once and there is no second query. Every walk is
// bounded. Pure, so `scripts/check-passwords-links.ts` runs it without vue.
import { sortByDate, type SortableHeader } from './sortFilter.ts'

/** The part of a header the links read. */
export type LinkHeader = SortableHeader & {
  parentId?: string | null
  groupId?: string | null
}

/**
 * What a pass says about the entry it belongs to: nothing; an entry that is there (`open`) or in
 * the trash; one that is not in the vault (it may still arrive by sync); or the pass is part of a
 * loop of links.
 */
export type ParentState<T extends LinkHeader = LinkHeader> =
  | { state: 'none' }
  | { state: 'open' | 'in_trash'; parent: T }
  | { state: 'not_available'; parentId: string }
  | { state: 'loop' }

/**
 * The ids of every pass that lies on a loop of parent links. A pass that merely leads into a loop
 * is not on it. Each header is visited once, so a long chain or hundreds of children stay cheap.
 */
export function loopMembers(headers: readonly LinkHeader[]): Set<string> {
  const parentOf = new Map<string, string>()
  for (const header of headers) {
    if (header.parentId) parentOf.set(header.id, header.parentId)
  }
  const onLoop = new Set<string>()
  const done = new Set<string>()
  for (const header of headers) {
    if (done.has(header.id)) continue
    const path: string[] = []
    const position = new Map<string, number>()
    let current: string | undefined = header.id
    while (current !== undefined && !done.has(current)) {
      const seen = position.get(current)
      if (seen !== undefined) {
        for (const id of path.slice(seen)) onLoop.add(id)
        break
      }
      position.set(current, path.length)
      path.push(current)
      current = parentOf.get(current)
    }
    for (const id of path) done.add(id)
  }
  return onLoop
}

/** The state of the parent of `header`; see [`ParentState`]. */
export function parentState<T extends LinkHeader>(
  header: T,
  headers: readonly T[],
  trashGroups: ReadonlySet<string>,
): ParentState<T> {
  if (!header.parentId) return { state: 'none' }
  if (loopMembers(headers).has(header.id)) return { state: 'loop' }
  const parent = headers.find((candidate) => candidate.id === header.parentId)
  if (!parent) return { state: 'not_available', parentId: header.parentId }
  return {
    state:
      parent.groupId && trashGroups.has(parent.groupId) ? 'in_trash' : 'open',
    parent,
  }
}

/**
 * The passes that belong directly to `id`, in the order of FR-021. A pass in the trash is not
 * listed (it is not one of the entry's passes any more until it is restored).
 */
export function linkedOf<T extends LinkHeader>(
  id: string,
  headers: readonly T[],
  now: Date,
  trashGroups: ReadonlySet<string>,
): T[] {
  return sortByDate(
    headers.filter(
      (header) =>
        header.parentId === id &&
        !(header.groupId && trashGroups.has(header.groupId)),
    ),
    now,
  )
}

/** The host of an address typed with or without a scheme, without `www.`; `null` if there is none. */
function hostOf(address: string | null | undefined): string | null {
  const text = address?.trim()
  if (!text) return null
  try {
    const url = new URL(
      /^[a-z][a-z0-9+.-]*:\/\//i.test(text) ? text : `https://${text}`,
    )
    return url.hostname.replace(/^www\./i, '').toLowerCase() || null
  } catch {
    return null
  }
}

/** The shortest name that counts as a match: "DB" would match every word that holds it. */
const MIN_NAME_MATCH = 3

/**
 * The entries a new pass most probably belongs to, first: those with the same address host, or
 * whose title holds the pass's title or the other way round. The rest keep their order. Nothing
 * is linked by this; the user picks (FR-016).
 */
export function suggestParents<T extends LinkHeader & { url?: string | null }>(
  wanted: { title?: string | null; url?: string | null },
  candidates: readonly T[],
): { probable: T[]; others: T[] } {
  const host = hostOf(wanted.url)
  const title = wanted.title?.trim().toLowerCase() ?? ''
  const probable: T[] = []
  const others: T[] = []
  for (const candidate of candidates) {
    const theirTitle = candidate.title?.trim().toLowerCase() ?? ''
    const sameHost = host !== null && hostOf(candidate.url) === host
    const sameName =
      title.length >= MIN_NAME_MATCH &&
      theirTitle.length >= MIN_NAME_MATCH &&
      (theirTitle.includes(title) || title.includes(theirTitle))
    ;(sameHost || sameName ? probable : others).push(candidate)
  }
  return { probable, others }
}
