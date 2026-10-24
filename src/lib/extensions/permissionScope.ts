// Which parts of an extension permission are a wildcard that allows everything of its kind
// (contracts/permissions.md §Arten). Pure: `components/extensions/PermissionSummary.vue` shows it,
// `scripts/check-extensions-permission-scope.ts` tests it.

/**
 * The key under `extensions.permissions.everything` that names what a target `*` allows, or
 * `null` when the target is not such a wildcard. `notifications` only knows the target `*`, so it
 * allows nothing beyond the kind itself, and so does `peers`; `database` never accepts `*`.
 */
export function everythingKey(
  kind: string,
  action: string,
  target: string,
): string | null {
  if (
    target !== '*' ||
    kind === 'notifications' ||
    kind === 'peers' ||
    kind === 'database'
  ) {
    return null
  }
  return kind === 'remoteStorage' && action === 'add'
    ? 'remoteStorageAdd'
    : kind
}

/** Whether the action allows every action of its kind: only `web` has one (`*`, every method). */
export function isEveryAction(kind: string, action: string): boolean {
  return kind === 'web' && action === '*'
}

/** Whether the target says anything: the only target of `notifications` and `peers` is `*`. */
export function hasTarget(kind: string): boolean {
  return kind !== 'notifications' && kind !== 'peers'
}
