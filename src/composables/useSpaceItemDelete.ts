import { useStorageSharing } from '@/composables/useStorageSharing'

/**
 * Kinds of space-overview items that can be deleted via the shared
 * delete-affordance. Extend this union as new deletable item types are
 * introduced (see design doc §8).
 */
export type SpaceItemType =
  | 'shared_cloud_storage'
  | 'shared_file'
  | 'extension_grant'

/**
 * Context passed to a `SpaceItemDeleteHandler`. `itemId` is the primary
 * identifier the handler needs (e.g. shared-backend id for
 * `shared_cloud_storage`); `spaceId` is provided for handlers that need it.
 * `label` is an optional display string for confirmation UI in the caller.
 */
export interface SpaceItemDeleteContext {
  itemType: SpaceItemType
  itemId: string
  spaceId: string
  label?: string
}

export type SpaceItemDeleteHandler = (
  ctx: SpaceItemDeleteContext,
) => Promise<void>

/**
 * Thrown by `deleteItem` when no handler has been registered for the given
 * `itemType`. Carries a machine-readable `code = 'not_supported'` so callers
 * (e.g. J2's toast) can distinguish this from arbitrary handler failures
 * without relying on message text or `instanceof` alone.
 */
export class SpaceItemNotSupportedError extends Error {
  readonly code = 'not_supported' as const
  readonly itemType: SpaceItemType

  constructor(itemType: SpaceItemType) {
    super(`No delete handler registered for item type '${itemType}'`)
    this.name = 'SpaceItemNotSupportedError'
    this.itemType = itemType
  }
}

/**
 * Module-level registry — deliberately a singleton so every consumer of
 * `useSpaceItemDelete()` sees the same handlers. Composables that only need
 * to *dispatch* (e.g. J2's item row) don't have to know about registration
 * order.
 */
const registry = new Map<SpaceItemType, SpaceItemDeleteHandler>()

let defaultHandlersRegistered = false

/**
 * Auto-register the always-available handlers on first use. Kept idempotent
 * so re-imports (HMR, test resets) don't warn about duplicates.
 */
function ensureDefaultHandlers(): void {
  if (defaultHandlersRegistered) return
  defaultHandlersRegistered = true

  const { revokeBackend } = useStorageSharing()
  registry.set('shared_cloud_storage', async (ctx) => {
    await revokeBackend(ctx.itemId)
  })
}

/**
 * Composable exposing the space-item delete registry.
 *
 * - `registerHandler` — install (or replace) the handler for a given type.
 *   Replacing an existing handler emits a `console.warn` because it usually
 *   signals a bug (two modules claiming the same type).
 * - `deleteItem` — dispatch to the registered handler; throws
 *   `SpaceItemNotSupportedError` if none is registered.
 * - `hasHandler` — cheap check used by UI to decide whether to render the
 *   delete affordance at all.
 */
export function useSpaceItemDelete() {
  ensureDefaultHandlers()

  const registerHandler = (
    type: SpaceItemType,
    handler: SpaceItemDeleteHandler,
  ): void => {
    if (registry.has(type)) {
      console.warn(
        `[useSpaceItemDelete] Replacing existing handler for item type '${type}'`,
      )
    }
    registry.set(type, handler)
  }

  const deleteItem = async (ctx: SpaceItemDeleteContext): Promise<void> => {
    const handler = registry.get(ctx.itemType)
    if (!handler) {
      throw new SpaceItemNotSupportedError(ctx.itemType)
    }
    await handler(ctx)
  }

  const hasHandler = (type: SpaceItemType): boolean => registry.has(type)

  return { registerHandler, deleteItem, hasHandler }
}

/**
 * Test-only reset — clears the registry and forces default handlers to be
 * re-registered on the next `useSpaceItemDelete()` call. Not exported from
 * the public API surface of the composable; callers should import it
 * explicitly.
 */
export function __resetSpaceItemDeleteRegistryForTests(): void {
  registry.clear()
  defaultHandlersRegistered = false
}
