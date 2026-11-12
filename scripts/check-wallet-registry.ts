// Part of `pnpm check:wallet` (spec 092-wallet-passes, Phase W): the places of the Wallet app
// (src/lib/wallet/registry.ts). Every place resolves, no place can carry a title, a code or another
// value, the links from other apps are built from opaque ids only, and the app is in the registry
// of apps.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import { readFileSync } from 'node:fs'
import { WM_APPS } from '../src/lib/wm/apps.ts'
import {
  createHistory,
  formatLocation,
  parseLocation,
  push,
  type TabHistory,
} from '../src/lib/wm/navigation.ts'
import { openApp } from '../src/lib/wm/layoutState.ts'
import { snapshotSession } from '../src/lib/wm/session.ts'
import { matchRoute } from '../src/lib/wm/routeMatch.ts'
import {
  isWalletSecretFreeLocation,
  walletEntryPath,
  walletLocationFor,
  walletNewPassLocation,
  WALLET_LOCATIONS,
  walletRoutePatterns,
} from '../src/lib/wallet/registry.ts'
import { ALPHA, APPS, emptyState } from './lib/wm-fixtures.ts'

const MARKER = 'SECRET-MARKER-WALLET'
/** A planted value is no id: it has a space, as a title would. */
const NOT_AN_ID = `${MARKER} login`

const SAMPLE_PATHS: Record<string, string> = {
  list: '/',
  folder: '/folder/3f2a-9c',
  entry: '/entry/0b1e5c7a-1',
  history: '/entry/0b1e5c7a-1/history',
  trash: '/trash',
}

test('Wallet is an app of its own: several windows, the tab shows the app name', () => {
  const app = WM_APPS.find((candidate) => candidate.id === 'system.wallet')
  assert.ok(app, 'system.wallet is registered')
  assert.equal(app.titleKey, 'wm.apps.wallet')
  assert.equal(app.icon, 'lucide:wallet')
  assert.equal(app.multiInstance, true)
  assert.equal(app.tabTitle, 'app')
  assert.equal(WM_APPS.filter((candidate) => candidate.id === app.id).length, 1)
})

test('every place resolves through matchRoute to its own location', () => {
  for (const location of WALLET_LOCATIONS) {
    const path = SAMPLE_PATHS[location.id]
    assert.ok(path, `a sample path exists for ${location.id}`)
    assert.ok(matchRoute(walletRoutePatterns(), path), `${path} matches`)
    assert.equal(walletLocationFor(path)?.location.id, location.id)
  }
})

test('a path that is no place does not resolve', () => {
  assert.equal(walletLocationFor('/entry'), undefined)
  assert.equal(walletLocationFor('/entry/a/b'), undefined)
  assert.equal(walletLocationFor('/folder'), undefined)
  assert.equal(walletLocationFor('/folder/a/b'), undefined)
  assert.equal(walletLocationFor('/nothing'), undefined)
})

test('the title of every place is a fixed key, never a stored value', () => {
  for (const location of WALLET_LOCATIONS) {
    assert.match(location.titleKey, /^wm\.wallet\.[a-z]+$/)
  }
})

test('secret-free locations: ids, the search text, the kind, edit, tab and parent only', () => {
  const free = (path: string, query: Record<string, string> = {}) =>
    isWalletSecretFreeLocation({ path, query })
  assert.ok(free('/'))
  assert.ok(free('/', { q: 'bahn' }))
  assert.ok(free('/', { kind: 'ticket' }))
  assert.ok(free('/entry/abc-1', { edit: '' }))
  assert.ok(free('/entry/abc-1', { tab: 'extra' }))
  assert.ok(free('/entry/new', { parent: 'abc-1' }))
  assert.ok(free('/entry/new', { folder: 'f-1', parent: 'abc-1' }))
  assert.ok(free('/folder/f-1', { kind: 'voucher', q: 'x' }))

  assert.ok(!free('/', { kind: MARKER }), 'a kind is one of the fixed ones')
  assert.ok(!free('/', { q: 'x'.repeat(201) }))
  assert.ok(
    !free(`/entry/${NOT_AN_ID}`),
    'an id with spaces or symbols is no id',
  )
  assert.ok(
    !free('/entry/abc-1', { parent: 'abc-2' }),
    'parent is for a new pass',
  )
  assert.ok(!free('/entry/abc-1', { title: MARKER }), 'no other key')
  assert.ok(!free('/entry/abc-1', { tab: 'history' }))
  assert.ok(!free('/entry/abc-1', { kind: 'ticket' }), 'kind is for the list')
  assert.ok(!free('/', { tag: 'abc' }), 'Wallet has no tag filter')
  assert.ok(
    !free('/entry/abc-1', { folder: 'f-1' }),
    'folder is for a new pass',
  )
  assert.ok(!free('/entry/new', { folder: NOT_AN_ID }))
})

test('a link from another app is built from an opaque id, or refused', () => {
  assert.equal(walletEntryPath('0b1e5c7a-1'), '/entry/0b1e5c7a-1')
  assert.equal(walletEntryPath(NOT_AN_ID), null)
  assert.equal(walletEntryPath(''), null)
  assert.equal(walletEntryPath('../trash'), null)
  assert.deepEqual(walletNewPassLocation('abc-1'), {
    path: '/entry/new',
    query: { parent: 'abc-1' },
  })
  assert.deepEqual(walletNewPassLocation('abc-1', 'f-1'), {
    path: '/entry/new',
    query: { parent: 'abc-1', folder: 'f-1' },
  })
  assert.deepEqual(walletNewPassLocation(), { path: '/entry/new', query: {} })
  assert.deepEqual(walletNewPassLocation(NOT_AN_ID), {
    path: '/entry/new',
    query: {},
  })
})

test('a tab history of all places goes through the saved session without a value', () => {
  const state = emptyState()
  openApp(state, ALPHA.id, APPS)
  const tab = state.windows[0]!.tabs[0]!
  let history: TabHistory = createHistory('/')
  for (const path of Object.values(SAMPLE_PATHS)) {
    // Titles are the static place titles, never a pass value.
    const title = walletLocationFor(path)?.location.titleKey ?? null
    history = push(history, path, title)
  }
  for (const entry of history.entries) {
    assert.ok(isWalletSecretFreeLocation(entry.location), entry.location.path)
  }
  const session = snapshotSession(state, (id) =>
    id === tab.id ? history : undefined,
  )
  const text = JSON.stringify(session)
  assert.equal(text.includes(MARKER), false)
  assert.ok(text.includes('/entry/0b1e5c7a-1/history'))
})

test('a location survives the formatted text round trip and stays secret free', () => {
  const location = { path: '/', query: { q: 'bahn', kind: 'ticket' } }
  const parsed = parseLocation(formatLocation(location))
  assert.deepEqual(parsed, location)
  assert.ok(isWalletSecretFreeLocation(parsed))
})

// The locale files stay in lockstep for what Wallet introduces: every key exists in German and
// English, and no value is empty.
function flatten(value: unknown, prefix = ''): Map<string, string> {
  const out = new Map<string, string>()
  if (value && typeof value === 'object') {
    for (const [key, child] of Object.entries(value)) {
      const path = prefix ? `${prefix}.${key}` : key
      if (child && typeof child === 'object') {
        for (const [k, v] of flatten(child, path)) out.set(k, v)
      } else {
        out.set(path, String(child))
      }
    }
  }
  return out
}

function locale(name: 'de' | 'en'): Map<string, string> {
  const url = new URL(`../src/i18n/locales/${name}.json`, import.meta.url)
  return flatten(JSON.parse(readFileSync(url, 'utf8')))
}

test('the German and English texts of Wallet have the same keys, none empty', () => {
  const de = locale('de')
  const en = locale('en')
  const owned = (key: string) =>
    key === 'wm.apps.wallet' ||
    key.startsWith('wm.wallet.') ||
    key.startsWith('wallet.')
  const deKeys = [...de.keys()].filter(owned).sort()
  const enKeys = [...en.keys()].filter(owned).sort()
  assert.ok(deKeys.length > 0)
  assert.deepEqual(deKeys, enKeys)
  for (const key of deKeys) {
    assert.ok((de.get(key) ?? '').trim() !== '', `de ${key}`)
    assert.ok((en.get(key) ?? '').trim() !== '', `en ${key}`)
  }
  for (const location of WALLET_LOCATIONS) {
    assert.ok(de.has(location.titleKey), location.titleKey)
  }
  assert.ok(de.has('wm.apps.wallet'))
})
