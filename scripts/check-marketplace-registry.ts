// Part of `pnpm check:marketplace` (spec 069): the places of the marketplace app are the catalog
// and the details of a slug; titles are fixed keys and a path never carries what a marketplace
// sent beyond the slug of the entry.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  MARKETPLACE_LOCATIONS,
  detailsPath,
  marketplaceLocationFor,
} from '../src/lib/marketplace/registry.ts'
import { WM_APPS, getAppDefinition } from '../src/lib/wm/apps.ts'

test('the app is a singleton window whose tab shows the app name', () => {
  const app = getAppDefinition('system.marketplace')
  assert.ok(app)
  assert.equal(app.titleKey, 'wm.apps.marketplace')
  assert.equal(app.multiInstance, false)
  assert.equal(app.tabTitle, 'app')
  assert.deepEqual(app.defaultSize, { width: 960, height: 640 })
  assert.deepEqual(app.minSize, { width: 360, height: 360 })
  assert.equal(
    WM_APPS.filter((candidate) => candidate.id === 'system.marketplace').length,
    1,
  )
})

test('the catalog is the root and a slug leads to the details', () => {
  assert.equal(marketplaceLocationFor('/')?.location.id, 'catalog')
  const details = marketplaceLocationFor('/haex-tetris')
  assert.equal(details?.location.id, 'details')
  assert.equal(details?.params.slug, 'haex-tetris')
  assert.equal(marketplaceLocationFor('/a/b'), undefined)
})

test('every place has a fixed title key', () => {
  assert.deepEqual(
    MARKETPLACE_LOCATIONS.map((location) => location.titleKey),
    ['wm.marketplace.catalog', 'wm.marketplace.details'],
  )
})

test('the path of an entry carries the slug and the source id, encoded', () => {
  assert.equal(
    detailsPath('preset-public', 'haex-tetris'),
    '/haex-tetris?source=preset-public',
  )
  assert.equal(detailsPath('a b', 'x/y'), '/x%2Fy?source=a%20b')
})
