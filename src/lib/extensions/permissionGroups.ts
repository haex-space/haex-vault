// The declared permissions of an extension grouped by kind for the install dialog (spec 017
// FR-067). Pure: `components/extensions/InstallDialog.vue` shows the groups,
// `scripts/check-extensions-permission-groups.ts` tests it.
import { everythingKey, isEveryAction } from './permissionScope.ts'

/** The part of a permission the grouping reads. */
export interface GroupablePermission {
  kind: string
  action: string
  target: string
}

export interface PermissionGroup<T extends GroupablePermission> {
  kind: string
  rows: T[]
  /** Some row allows everything of its kind (a target `*` or the web action `*`). */
  wide: boolean
}

/** Whether the permission is a wildcard that allows everything of its kind. */
export function hasWildcard(row: GroupablePermission): boolean {
  return (
    everythingKey(row.kind, row.action, row.target) !== null ||
    isEveryAction(row.kind, row.action)
  )
}

/** The permissions by kind, the groups in the order their kind first appears. */
export function groupPermissions<T extends GroupablePermission>(
  rows: readonly T[],
): PermissionGroup<T>[] {
  const groups = new Map<string, PermissionGroup<T>>()
  for (const row of rows) {
    let group = groups.get(row.kind)
    if (!group) {
      group = { kind: row.kind, rows: [], wide: false }
      groups.set(row.kind, group)
    }
    group.rows.push(row)
    group.wide ||= hasWildcard(row)
  }
  return [...groups.values()]
}
