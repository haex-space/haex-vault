import { vi } from 'vitest'

/**
 * A drizzle query-builder chain is both awaitable (resolves to the row list)
 * and chainable (`.from()/.where()/.limit()/.set()/.values()` all return the
 * same object). Attaching the chain methods directly to a real `Promise`
 * lets handler code `await db.select(...).from(...).where(...)` without the
 * mock needing to model drizzle's actual SQL builder.
 */
export function queryResult<T>(rows: T): Promise<T> & Record<string, ReturnType<typeof vi.fn>> {
  const promise = Promise.resolve(rows) as Promise<T> & Record<string, ReturnType<typeof vi.fn>>
  promise.from = vi.fn(() => promise)
  promise.where = vi.fn(() => promise)
  promise.limit = vi.fn(() => promise)
  promise.set = vi.fn(() => promise)
  promise.values = vi.fn(() => promise)
  return promise
}

export interface MockDb {
  select: ReturnType<typeof vi.fn>
  insert: ReturnType<typeof vi.fn>
  update: ReturnType<typeof vi.fn>
  delete: ReturnType<typeof vi.fn>
}

export function createMockDb(): MockDb {
  return {
    select: vi.fn(),
    insert: vi.fn(),
    update: vi.fn(),
    delete: vi.fn(),
  }
}
