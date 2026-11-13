// Which app shows an entry (spec 092-wallet-passes, Phase W): the password manager shows logins, and
// whatever type a newer holzi wrote, which it must not hide; Wallet shows the cards and tickets.
// The store keeps every header, these pure functions cut the views. `scripts/check-wallet-scope.ts`
// runs them without vue.
import { walletGroupIds } from '../wallet/folders.ts'
import { entryTypeOf, type TypedHeader } from './passType.ts'

export type AppScope = 'login' | 'pass'

/** The scope an entry belongs to: `pass` for a card or ticket, `login` for everything else. */
export function scopeOf(header: TypedHeader | null | undefined): AppScope {
  return entryTypeOf(header) === 'pass' ? 'pass' : 'login'
}

/** The headers of one scope, in their order. */
export function inScope<T extends TypedHeader>(
  headers: readonly T[],
  scope: AppScope,
): T[] {
  return headers.filter((header) => scopeOf(header) === scope)
}

type TagOfHeader = { id: string }
type ScopedTag = { ids: readonly string[]; itemCount: number }

/**
 * The tags the password manager lists: counted over its logins only, and a tag that only cards or
 * tickets carry is left out (it would filter to an empty list). A tag nothing carries yet stays, as
 * before.
 */
export function loginTagsOf<T extends ScopedTag>(
  tags: readonly T[],
  headers: readonly (TypedHeader & { tags: readonly TagOfHeader[] })[],
): T[] {
  const onPasses = new Set<string>()
  const logins = inScope(headers, 'login')
  for (const header of inScope(headers, 'pass')) {
    for (const tag of header.tags) onPasses.add(tag.id)
  }
  return tags
    .map((tag) => {
      const wanted = new Set(tag.ids)
      return {
        ...tag,
        itemCount: logins.filter((header) =>
          header.tags.some((carried) => wanted.has(carried.id)),
        ).length,
      }
    })
    .filter(
      (tag) => tag.itemCount > 0 || !tag.ids.some((id) => onPasses.has(id)),
    )
}

type TrashHeader = TypedHeader & { id: string; groupId: string | null }
type TrashGroup = {
  id: string
  parentId: string | null
  trashedFromParentId?: string | null
}
export type TrashTarget = { kind: 'group' | 'item'; id: string }

/** The ids of a folder and every folder below it. */
function subtreeOf(rootId: string, groups: readonly TrashGroup[]): Set<string> {
  const ids = new Set<string>([rootId])
  let grew = true
  while (grew) {
    grew = false
    for (const group of groups) {
      if (group.parentId && ids.has(group.parentId) && !ids.has(group.id)) {
        ids.add(group.id)
        grew = true
      }
    }
  }
  return ids
}

/** What a removal for good takes: the targets to send, the entries and folders that go with them,
 * and the ids of those entries (for the references other entries hold on them). */
export type TrashPlan = {
  targets: TrashTarget[]
  entries: number
  folders: number
  itemIds: string[]
}

const item = (id: string): TrashTarget => ({ kind: 'item', id })

/**
 * What "Empty trash" removes in one scope. One trash holds both apps' entries and folders, so
 * neither app may empty the other's. Wallet's folders deleted directly go whole with the passes in
 * them; a pass is also Wallet's wherever it lies below the trash outside a Wallet folder (a pass
 * can sit in a password-manager folder from before Wallet), so it goes one by one. The password
 * manager removes a folder deleted directly only when no pass lies inside it; a folder that holds
 * one stays, and only the logins in it go, one by one.
 */
export function emptyPlan(
  scope: AppScope,
  trashId: string,
  groups: readonly TrashGroup[],
  headers: readonly TrashHeader[],
): TrashPlan {
  const wallet = walletGroupIds(groups)
  const inTrash = subtreeOf(trashId, groups)
  const mine = inScope(headers, scope).filter(
    (header) => header.groupId !== null && inTrash.has(header.groupId),
  )
  const itemIds = mine.map((header) => header.id)
  const passes = inScope(headers, 'pass')
  const targets: TrashTarget[] = []
  const goWithFolder = new Set<string>()
  let folders = 0
  const direct = groups.filter(
    (g) => g.parentId === trashId && wallet.has(g.id) === (scope === 'pass'),
  )
  for (const group of direct) {
    const inside = subtreeOf(group.id, groups)
    if (
      scope === 'login' &&
      passes.some((p) => p.groupId !== null && inside.has(p.groupId))
    )
      continue
    targets.push({ kind: 'group', id: group.id })
    folders += inside.size
    for (const id of inside) goWithFolder.add(id)
  }
  for (const header of mine) {
    if (!goWithFolder.has(header.groupId ?? '')) targets.push(item(header.id))
  }
  return { targets, entries: itemIds.length, folders, itemIds }
}

/**
 * What "Delete for good" on one row of the trash removes in one scope. An entry goes as it is. A
 * folder of the app goes as a whole, except a password-manager folder with a pass inside (that is
 * Wallet's to delete): then only the logins in it go and the folder stays. A folder of the other
 * app is never a target.
 */
export function deletePlan(
  scope: AppScope,
  target: TrashTarget,
  groups: readonly TrashGroup[],
  headers: readonly TrashHeader[],
): { targets: TrashTarget[]; itemIds: string[] } {
  if (target.kind === 'item') return { targets: [target], itemIds: [target.id] }
  if (walletGroupIds(groups).has(target.id) !== (scope === 'pass'))
    return { targets: [], itemIds: [] }
  const inside = subtreeOf(target.id, groups)
  const held = headers.filter(
    (header) => header.groupId !== null && inside.has(header.groupId),
  )
  if (scope === 'login' && held.some((header) => scopeOf(header) === 'pass')) {
    const logins = inScope(held, 'login').map((header) => header.id)
    return { targets: logins.map(item), itemIds: logins }
  }
  return { targets: [target], itemIds: held.map((header) => header.id) }
}

/** The apps that show entries. */
export type EntryApp = 'system.passwords' | 'system.wallet'

const OPAQUE_ID = /^[A-Za-z0-9-]{1,64}$/

/**
 * Where an entry opens: a card or ticket in Wallet, a login in the password manager (spec 092,
 * Phase W). A link between the two apps ("Linked", "Belongs to") follows it. `null` for an id that is
 * no opaque id, so a link can never carry a value.
 */
export function entryTarget(
  header: (TypedHeader & { id: string }) | null | undefined,
): { appId: EntryApp; path: string } | null {
  if (!header || !OPAQUE_ID.test(header.id)) return null
  return {
    appId: scopeOf(header) === 'pass' ? 'system.wallet' : 'system.passwords',
    path: `/entry/${header.id}`,
  }
}
