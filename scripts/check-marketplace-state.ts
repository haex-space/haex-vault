// Part of `pnpm check:marketplace` (spec 069): what a tile offers and which state the catalog
// shows are decided by pure rules.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  catalogQueryOf,
  catalogState,
} from '../src/lib/marketplace/catalogState.ts'
import { isCancelled } from '../src/lib/marketplace/errors.ts'
import { selectable, tileAction } from '../src/lib/marketplace/tileState.ts'

const source = { id: 'preset-public', name: 'Haex Marketplace' }

const version = (
  number: string,
  extra: Record<string, unknown> = {},
): Record<string, unknown> => ({
  version: number,
  bundleSize: 100,
  compatible: true,
  ...extra,
})

const entry = (over: Record<string, unknown> = {}) =>
  ({
    source,
    extensionId: 'haex/demo',
    name: 'Demo',
    slug: 'demo',
    shortDescription: 'A demo',
    verified: false,
    totalDownloads: 0,
    reviewCount: 0,
    tags: [],
    versions: [version('1.0.0')],
    latestCompatible: '1.0.0',
    ...over,
  }) as never

test('an entry that is not installed offers its newest compatible version', () => {
  assert.deepEqual(tileAction(entry()), { kind: 'install', version: '1.0.0' })
})

test('an installed entry with a newer compatible version offers the update', () => {
  assert.deepEqual(
    tileAction(
      entry({
        installed: { version: '1.0.0', enabled: true },
        updateTo: '1.2.0',
        latestCompatible: '1.2.0',
      }),
    ),
    { kind: 'update', from: '1.0.0', to: '1.2.0' },
  )
})

test('an installed entry without a newer version only says what is installed', () => {
  assert.deepEqual(
    tileAction(entry({ installed: { version: '1.0.0', enabled: false } })),
    { kind: 'installed', version: '1.0.0' },
  )
})

test('an entry with no compatible version names why, from the newest version', () => {
  const blocked = entry({
    latestCompatible: undefined,
    versions: [
      version('2.0.0', {
        compatible: false,
        incompatibleReason: 'needs_newer',
        bound: '9.0.0',
      }),
      version('1.0.0', {
        compatible: false,
        incompatibleReason: 'needs_newer',
      }),
    ],
  })
  assert.deepEqual(tileAction(blocked), {
    kind: 'blocked',
    reason: 'needs_newer',
    bound: '9.0.0',
  })
  assert.deepEqual(
    tileAction(entry({ latestCompatible: undefined, versions: [] })),
    {
      kind: 'blocked',
      reason: 'no_versions',
    },
  )
  assert.equal(
    (
      tileAction(
        entry({
          latestCompatible: undefined,
          versions: [
            version('1.0.0', {
              compatible: false,
              incompatibleReason: 'no_checksum',
            }),
          ],
        }),
      ) as { reason: string }
    ).reason,
    'no_checksum',
  )
})

test('an incompatible version cannot be chosen, an older compatible one can', () => {
  assert.equal(selectable(version('0.9.0') as never), true)
  assert.equal(
    selectable(version('2.0.0', { compatible: false }) as never),
    false,
  )
})

const market = (id: string, enabled: boolean) =>
  ({
    id,
    name: id,
    baseUrl: 'https://x.example',
    enabled,
    rank: 1,
    authType: 'none',
    credentials: 'none',
  }) as never

const page = (entries: unknown[], errors: unknown[] = []) =>
  ({
    entries,
    page: 1,
    totalPages: 1,
    total: entries.length,
    sourceErrors: errors,
  }) as never

test('the catalog state follows the rules in order', () => {
  assert.equal(catalogState({ markets: null, page: null }), 'loading')
  assert.equal(catalogState({ markets: [], page: null }), 'none')
  assert.equal(
    catalogState({ markets: [market('a', false)], page: null }),
    'allOff',
  )
  assert.equal(
    catalogState({ markets: [market('a', true)], page: null }),
    'loading',
  )
  assert.equal(
    catalogState({ markets: [market('a', true)], page: page([entry()]) }),
    'ready',
  )
  assert.equal(
    catalogState({ markets: [market('a', true)], page: page([]) }),
    'empty',
  )
})

test('offline means every asked marketplace failed; one that failed beside an answer is not offline', () => {
  const error = (id: string) => ({
    sourceId: id,
    name: id,
    kind: 'unreachable',
  })
  const two = [market('a', true), market('b', true), market('c', false)]
  assert.equal(
    catalogState({
      markets: two,
      page: page([], [error('a'), error('b')]),
    }),
    'offline',
  )
  assert.equal(
    catalogState({ markets: two, page: page([], [error('a')]) }),
    'empty',
  )
  assert.equal(
    catalogState({
      markets: two,
      page: page([], [error('b')]),
      sourceId: 'b',
    }),
    'offline',
    'a source filter asks only that marketplace',
  )
  assert.equal(
    catalogState({ markets: two, page: page([]), sourceId: 'stale' }),
    'empty',
    'a stale filter with no source error is not offline',
  )
  assert.equal(
    catalogState({
      markets: two,
      page: page([entry()], [error('a'), error('b')]),
    }),
    'ready',
    'an entry is shown even beside errors',
  )
})

test('the route query becomes the catalog query, an empty value is no filter', () => {
  assert.deepEqual(catalogQueryOf({}), {
    page: 1,
    search: undefined,
    category: undefined,
    sourceId: undefined,
  })
  assert.deepEqual(
    catalogQueryOf({
      q: ' tetris ',
      category: 'games',
      source: 'm1',
      page: '3',
    }),
    { page: 3, search: 'tetris', category: 'games', sourceId: 'm1' },
  )
  assert.equal(catalogQueryOf({ page: '0' }).page, 1)
  assert.equal(catalogQueryOf({ page: 'x' }).page, 1)
  assert.equal(catalogQueryOf({ q: '   ' }).search, undefined)
})

test('a cancelled download or a replaced catalog call is not reported as a failure', () => {
  assert.equal(
    isCancelled({
      kind: 'Marketplace',
      reason: 'marketplace_download_cancelled',
    }),
    true,
  )
  assert.equal(
    isCancelled({ kind: 'Marketplace', reason: 'marketplace_superseded' }),
    true,
  )
  assert.equal(
    isCancelled({ kind: 'Marketplace', reason: 'marketplace_hash_mismatch' }),
    false,
  )
  assert.equal(isCancelled('x'), false)
  assert.equal(isCancelled(null), false)
})
