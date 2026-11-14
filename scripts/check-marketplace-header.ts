// Part of `pnpm check:marketplace` (spec 093-app-headers, US4): the catalogue's header description and
// the commands its events stand for (`src/lib/marketplace/header.ts`).
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

import {
  marketplaceHeader,
  marketplaceHeaderCommand,
  type MarketplaceHeaderState,
} from '../src/lib/marketplace/header.ts'
import { headerProblems } from '../src/lib/wm/headerChecks.ts'
import type { WmHeaderControl } from '../src/types/bindings/WmHeaderControl.ts'

/** A translation that shows its key, so a test sees which text a control carries. */
const t = (key: string) => key

const STATE: MarketplaceHeaderState = {
  query: '',
  categories: [
    { slug: 'tools', name: 'Tools', count: 5 },
    { slug: 'games', name: 'Games', count: 2 },
  ],
  category: '',
  markets: [
    { id: 'main', name: 'Main', enabled: true },
    { id: 'team', name: 'Team', enabled: true },
    { id: 'off', name: 'Off', enabled: false },
  ],
  source: '',
  loading: false,
}

const ids = (controls: readonly WmHeaderControl[] | undefined) =>
  (controls ?? []).map((c) => c.id)
const find = (state: MarketplaceHeaderState, id: string) => {
  const d = marketplaceHeader(state, t)
  return [...(d.start ?? []), ...(d.center ?? []), ...(d.end ?? [])].find(
    (c) => c.id === id,
  )
}
const itemsOf = (control: WmHeaderControl | undefined) =>
  control?.type === 'menu'
    ? control.items.flatMap((i) => (i.type === 'item' ? [i] : []))
    : []
const checked = (control: WmHeaderControl | undefined) =>
  itemsOf(control)
    .filter((i) => i.checked)
    .map((i) => i.id)

test('the catalogue has no sidebar button: search, category, source and refresh', () => {
  const d = marketplaceHeader(STATE, t)
  assert.deepEqual(ids(d.start), [])
  assert.deepEqual(ids(d.center), ['q'])
  assert.deepEqual(ids(d.end), ['category', 'source', 'refresh'])
})

test('the marketplace choice shows only with more than one marketplace switched on', () => {
  const one = {
    ...STATE,
    markets: [
      { id: 'main', name: 'Main', enabled: true },
      { id: 'off', name: 'Off', enabled: false },
    ],
  }
  assert.deepEqual(ids(marketplaceHeader(one, t).end), ['category', 'refresh'])
  assert.deepEqual(ids(marketplaceHeader({ ...STATE, markets: [] }, t).end), [
    'category',
    'refresh',
  ])
})

test('the category menu lists every category with its count, the current one checked', () => {
  const all = find(STATE, 'category')
  assert.deepEqual(
    itemsOf(all).map((i) => [i.id, i.label]),
    [
      ['all-categories', 'marketplace.category.all'],
      ['cat:tools', 'Tools (5)'],
      ['cat:games', 'Games (2)'],
    ],
  )
  assert.deepEqual(checked(all), ['all-categories'])
  assert.deepEqual(checked(find({ ...STATE, category: 'games' }, 'category')), [
    'cat:games',
  ])
})

test('the source menu lists the enabled marketplaces, the current one checked', () => {
  const source = find(STATE, 'source')
  assert.deepEqual(
    itemsOf(source).map((i) => [i.id, i.label]),
    [
      ['all-sources', 'marketplace.source.all'],
      ['src:main', 'Main'],
      ['src:team', 'Team'],
    ],
  )
  assert.deepEqual(checked(source), ['all-sources'])
  assert.deepEqual(checked(find({ ...STATE, source: 'team' }, 'source')), [
    'src:team',
  ])
})

test('a menu is active only while it is off "all"', () => {
  const active = (state: MarketplaceHeaderState, id: string) => {
    const c = find(state, id)
    return c?.type === 'menu' && c.active === true
  }
  assert.equal(active(STATE, 'category'), false)
  assert.equal(active(STATE, 'source'), false)
  assert.equal(active({ ...STATE, category: 'tools' }, 'category'), true)
  assert.equal(active({ ...STATE, source: 'main' }, 'source'), true)
  assert.equal(active({ ...STATE, source: 'main' }, 'category'), false)
})

test('refresh is disabled while the catalogue loads', () => {
  const idle = find(STATE, 'refresh')
  assert.ok(idle?.type === 'button')
  assert.notEqual(idle.disabled, true)
  const busy = find({ ...STATE, loading: true }, 'refresh')
  assert.ok(busy?.type === 'button')
  assert.equal(busy.disabled, true)
})

test('the search carries the text and gives way first, then the choices, then refresh', () => {
  const q = find({ ...STATE, query: 'agent' }, 'q')
  assert.ok(q?.type === 'search')
  assert.equal(q.value, 'agent')
  assert.deepEqual(q.collapse, { order: 10, form: 'shrink' })
  const order = (id: string) => find(STATE, id)?.collapse?.order ?? 1001
  assert.ok(order('q') < order('category'))
  assert.ok(order('category') <= order('source'))
  assert.ok(order('source') < order('refresh'))
})

test('the controls of today keep their test ids', () => {
  assert.equal(find(STATE, 'q')?.testId, 'marketplace-search')
  assert.equal(find(STATE, 'category')?.testId, 'marketplace-category')
  assert.equal(find(STATE, 'source')?.testId, 'marketplace-source-filter')
  assert.equal(find(STATE, 'refresh')?.testId, 'marketplace-refresh')
})

test('a control id and an entry id are never the same, even for the two "all" entries', () => {
  const d = marketplaceHeader(STATE, t)
  const all = [...(d.start ?? []), ...(d.center ?? []), ...(d.end ?? [])]
  const seen = all.flatMap((c) => [
    c.id,
    ...(c.type === 'menu'
      ? c.items.flatMap((i) => (i.type === 'item' ? [i.id] : []))
      : []),
  ])
  assert.equal(new Set(seen).size, seen.length, seen.join(', '))
  assert.deepEqual(headerProblems(d), [])
})

test('events stand for the commands the toolbar had', () => {
  const cmd = marketplaceHeaderCommand
  assert.deepEqual(cmd({ id: 'q', kind: 'input', value: 'agent' }), {
    kind: 'search',
    text: 'agent',
  })
  assert.equal(cmd({ id: 'q', kind: 'submit', value: 'agent' }), null)
  assert.deepEqual(
    cmd({ id: 'category', kind: 'click', value: 'all-categories' }),
    {
      kind: 'category',
      slug: '',
    },
  )
  assert.deepEqual(cmd({ id: 'category', kind: 'click', value: 'cat:tools' }), {
    kind: 'category',
    slug: 'tools',
  })
  assert.deepEqual(cmd({ id: 'source', kind: 'click', value: 'all-sources' }), {
    kind: 'source',
    id: '',
  })
  assert.deepEqual(cmd({ id: 'source', kind: 'click', value: 'src:team' }), {
    kind: 'source',
    id: 'team',
  })
  assert.deepEqual(cmd({ id: 'refresh', kind: 'click' }), { kind: 'refresh' })
  assert.equal(cmd({ id: 'category', kind: 'click', value: 'zzz' }), null)
  assert.equal(cmd({ id: 'unknown', kind: 'click' }), null)
})

test('every text of the header exists in German and English', () => {
  const used = new Set<string>()
  const spy = (key: string) => {
    used.add(key)
    return key
  }
  for (const state of [STATE, { ...STATE, loading: true }])
    marketplaceHeader(state, spy)
  assert.ok(used.size >= 6, 'the header asked for its texts')
  for (const language of ['de', 'en']) {
    const messages: unknown = JSON.parse(
      readFileSync(
        new URL(`../src/i18n/locales/${language}.json`, import.meta.url),
        'utf8',
      ),
    )
    for (const key of used) {
      const text = key
        .split('.')
        .reduce<unknown>(
          (node, part) =>
            node && typeof node === 'object'
              ? (node as Record<string, unknown>)[part]
              : undefined,
          messages,
        )
      assert.equal(typeof text, 'string', `${language}: "${key}" is no text`)
    }
  }
})
