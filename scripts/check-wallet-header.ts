// Part of `pnpm check:wallet` (spec 093-app-headers, US3): Wallet's header description and the
// commands its events stand for (`src/lib/wallet/header.ts`).
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

import {
  walletHeader,
  walletHeaderCommand,
  type WalletHeaderState,
} from '../src/lib/wallet/header.ts'
import { headerProblems } from '../src/lib/wm/headerChecks.ts'
import type { WmHeaderControl } from '../src/types/bindings/WmHeaderControl.ts'

/** A translation that shows its key, so a test sees which text a control carries. */
const t = (key: string) => key

const STATE: WalletHeaderState = {
  sidebarVisible: true,
  passOpen: false,
  query: '',
  sort: 'date',
  kind: null,
  hideExpired: false,
}

const ids = (controls: readonly WmHeaderControl[] | undefined) =>
  (controls ?? []).map((c) => c.id)
const find = (state: WalletHeaderState, id: string) => {
  const d = walletHeader(state, t)
  return [...(d.start ?? []), ...(d.center ?? []), ...(d.end ?? [])].find(
    (c) => c.id === id,
  )
}
const checked = (control: WmHeaderControl | undefined) =>
  control?.type === 'menu'
    ? control.items.flatMap((i) =>
        i.type === 'item' && i.checked ? [i.id] : [],
      )
    : []

test('a list shows the sidebar button, the search, the filters, scan, import and "New"', () => {
  const d = walletHeader(STATE, t)
  assert.deepEqual(ids(d.start), ['sidebar'])
  assert.deepEqual(ids(d.center), ['q'])
  assert.deepEqual(ids(d.end), ['filters', 'capture', 'import', 'new'])
})

test('an open pass (or the trash) keeps the sidebar button only', () => {
  const d = walletHeader({ ...STATE, passOpen: true }, t)
  assert.deepEqual(ids(d.start), ['sidebar'])
  assert.deepEqual(ids(d.center), [])
  assert.deepEqual(ids(d.end), [])
})

test('the sidebar button is pressed while the sidebar shows, and never collapses', () => {
  const on = find(STATE, 'sidebar')
  assert.ok(on?.type === 'toggle')
  assert.equal(on.label, 'wallet.sidebar.hide')
  assert.deepEqual(on.collapse, { form: 'keep' })
  const off = find({ ...STATE, sidebarVisible: false }, 'sidebar')
  assert.ok(off?.type === 'toggle')
  assert.equal(off.pressed, false)
})

test('the filters mark the current sort and kind, and "hide expired" when it is on', () => {
  assert.deepEqual(checked(find(STATE, 'filters')), ['sort-date', 'kind-all'])
  assert.deepEqual(
    checked(
      find(
        { ...STATE, sort: 'title', kind: 'voucher', hideExpired: true },
        'filters',
      ),
    ),
    ['sort-title', 'kind-voucher', 'hide-expired'],
  )
})

test('the filters are active when anything filters the list or the sort is not the default', () => {
  const active = (state: WalletHeaderState) => {
    const filters = find(state, 'filters')
    return filters?.type === 'menu' && filters.active === true
  }
  assert.equal(active(STATE), false)
  assert.equal(active({ ...STATE, kind: 'ticket' }), true)
  assert.equal(active({ ...STATE, hideExpired: true }), true)
  assert.equal(active({ ...STATE, sort: 'title' }), true)
})

test('the filters list the sorts, "all kinds", every kind and "hide expired"', () => {
  const filters = find(STATE, 'filters')
  assert.ok(filters?.type === 'menu')
  assert.deepEqual(
    filters.items.flatMap((i) => (i.type === 'item' ? [i.id] : [])),
    [
      'sort-date',
      'sort-title',
      'kind-all',
      'kind-ticket',
      'kind-loyalty_card',
      'kind-voucher',
      'kind-membership',
      'kind-boarding_pass',
      'kind-other',
      'hide-expired',
    ],
  )
})

test('the controls of today keep their test ids; entries are <menu>-<entry>', () => {
  assert.equal(find(STATE, 'sidebar')?.testId, 'wallet-sidebar-toggle')
  assert.equal(find(STATE, 'q')?.testId, 'wallet-search')
  assert.equal(find(STATE, 'filters')?.testId, 'wallet-filters')
  assert.equal(find(STATE, 'capture')?.testId, 'wallet-capture')
  assert.equal(find(STATE, 'import')?.testId, 'wallet-import')
  assert.equal(find(STATE, 'new')?.testId, 'wallet-new')
})

test('the search gives way first, "New" last', () => {
  const order = (id: string) => find(STATE, id)?.collapse?.order ?? 1001
  assert.equal(order('q'), 10)
  for (const id of ['filters', 'capture', 'import'])
    assert.ok(order('q') < order(id) && order(id) < order('new'), id)
})

test('the description passes the header checks', () => {
  assert.deepEqual(headerProblems(walletHeader(STATE, t)), [])
  assert.deepEqual(
    headerProblems(walletHeader({ ...STATE, kind: 'ticket' }, t)),
    [],
  )
})

test('events stand for the commands the toolbar had', () => {
  const cmd = walletHeaderCommand
  assert.deepEqual(cmd({ id: 'sidebar', kind: 'toggle', value: false }), {
    kind: 'sidebar',
  })
  assert.deepEqual(cmd({ id: 'q', kind: 'input', value: 'bahn' }), {
    kind: 'search',
    text: 'bahn',
  })
  assert.equal(cmd({ id: 'q', kind: 'submit', value: 'bahn' }), null)
  assert.deepEqual(cmd({ id: 'filters', kind: 'click', value: 'sort-title' }), {
    kind: 'sort',
    sort: 'title',
  })
  assert.deepEqual(cmd({ id: 'filters', kind: 'click', value: 'kind-all' }), {
    kind: 'kind',
    value: null,
  })
  assert.deepEqual(
    cmd({ id: 'filters', kind: 'click', value: 'kind-voucher' }),
    { kind: 'kind', value: 'voucher' },
  )
  assert.deepEqual(
    cmd({ id: 'filters', kind: 'click', value: 'hide-expired' }),
    { kind: 'toggleHideExpired' },
  )
  assert.equal(cmd({ id: 'filters', kind: 'click', value: 'kind-zzz' }), null)
  assert.equal(cmd({ id: 'filters', kind: 'click', value: 'zzz' }), null)
  for (const id of ['capture', 'import', 'new'] as const)
    assert.deepEqual(cmd({ id, kind: 'click' }), { kind: id })
  assert.equal(cmd({ id: 'unknown', kind: 'click' }), null)
})

test('every text of the header exists in German and English', () => {
  const used = new Set<string>()
  const spy = (key: string) => {
    used.add(key)
    return key
  }
  for (const state of [
    STATE,
    { ...STATE, passOpen: true },
    { ...STATE, sidebarVisible: false },
  ])
    walletHeader(state, spy)
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

test('a control id and an entry id are never the same (they share one namespace)', () => {
  for (const state of [STATE, { ...STATE, passOpen: true }]) {
    const d = walletHeader(state, t)
    const all = [...(d.start ?? []), ...(d.center ?? []), ...(d.end ?? [])]
    const seen = all.flatMap((c) => [
      c.id,
      ...(c.type === 'menu'
        ? c.items.flatMap((i) => (i.type === 'item' ? [i.id] : []))
        : []),
    ])
    assert.equal(new Set(seen).size, seen.length, seen.join(', '))
  }
})
