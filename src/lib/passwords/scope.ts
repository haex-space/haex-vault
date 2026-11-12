// Which app shows an entry (spec 092-wallet-passes, Phase W): the password manager shows logins, and
// whatever type a newer holzi wrote, which it must not hide; Wallet shows the cards and tickets.
// The store keeps every header, these pure functions cut the views. `scripts/check-wallet-scope.ts`
// runs them without vue.
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
type TrashGroup = { id: string; parentId: string | null }

/**
 * What "Empty trash" removes in one scope: the folders deleted directly (the login scope only; Wallet
 * has none until it has folders of its own) and the entries of that scope deleted directly. One
 * trash holds both apps' entries, so neither app may empty the other's.
 */
export function emptyTargets(
  scope: AppScope,
  trashId: string,
  groups: readonly TrashGroup[],
  headers: readonly TrashHeader[],
): { kind: 'group' | 'item'; id: string }[] {
  return [
    ...(scope === 'login'
      ? groups
          .filter((group) => group.parentId === trashId)
          .map((group) => ({ kind: 'group' as const, id: group.id }))
      : []),
    ...inScope(headers, scope)
      .filter((header) => header.groupId === trashId)
      .map((header) => ({ kind: 'item' as const, id: header.id })),
  ]
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
