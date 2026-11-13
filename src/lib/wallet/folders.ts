// The folders of Wallet (spec 092-wallet-passes, Phase W): a subtree of the folder rows under the
// fixed root `wallet`, which appears with the first folder (no migration). The password manager
// shows none of it, Wallet shows only it. Pure, so `scripts/check-wallet-folders.ts` runs it
// without vue.

/** The id of the root of Wallet's folders; a row with a fixed id and no name. */
export const WALLET_GROUP_ID = 'wallet'
const TRASH_ID = 'trash'

/** The part of a folder row the subtree needs. */
export type WalletGroup = {
  id: string
  parentId: string | null
  trashedFromParentId?: string | null
}

/**
 * The ids of Wallet's part of the folder rows: the root, its folders at any depth, and the ones
 * of them that lie in the trash (a folder deleted directly has the trash as its parent and
 * remembers the folder it came from).
 */
export function walletGroupIds(groups: readonly WalletGroup[]): Set<string> {
  const ids = new Set<string>([WALLET_GROUP_ID])
  let grew = true
  while (grew) {
    grew = false
    for (const group of groups) {
      if (ids.has(group.id)) continue
      const parent = group.parentId
      const viaTrash =
        parent === TRASH_ID &&
        group.trashedFromParentId != null &&
        ids.has(group.trashedFromParentId)
      if ((parent !== null && ids.has(parent)) || viaTrash) {
        ids.add(group.id)
        grew = true
      }
    }
  }
  return ids
}

/** The folder rows the password manager shows: everything but Wallet's part. */
export function loginGroupsOf<T extends WalletGroup>(
  groups: readonly T[],
): T[] {
  const wallet = walletGroupIds(groups)
  return groups.filter((group) => !wallet.has(group.id))
}

/**
 * Wallet's live folders, ready for the tree: the root and what lies in the trash left out, the
 * folders at the top of Wallet given no parent, so the tree of the password manager (`buildTree`)
 * builds Wallet's from them too.
 */
export function walletFoldersOf<T extends WalletGroup>(
  groups: readonly T[],
): T[] {
  const wallet = walletGroupIds(groups)
  const trashed = new Set<string>([TRASH_ID])
  let grew = true
  while (grew) {
    grew = false
    for (const group of groups) {
      if (
        !trashed.has(group.id) &&
        group.parentId !== null &&
        trashed.has(group.parentId)
      ) {
        trashed.add(group.id)
        grew = true
      }
    }
  }
  return groups
    .filter(
      (group) =>
        wallet.has(group.id) &&
        group.id !== WALLET_GROUP_ID &&
        !trashed.has(group.id),
    )
    .map((group) =>
      group.parentId === WALLET_GROUP_ID ? { ...group, parentId: null } : group,
    )
}

/** The folder a Wallet tree row stands for: the real parent of a folder at the top is the root. */
export function realParentOf(parentId: string | null): string {
  return parentId ?? WALLET_GROUP_ID
}
