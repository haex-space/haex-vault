// Part of `pnpm check:passwords` (spec 034-password-manager, FR-039, FR-040): the places of the
// password manager (src/lib/passwords/registry.ts). Every place resolves, no place can carry a
// title or value, and a tab history of all places passes the saved session without a planted marker.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

import { openApp } from '../src/lib/wm/layoutState.ts'
import {
  createHistory,
  formatLocation,
  parseLocation,
  push,
  type TabHistory,
} from '../src/lib/wm/navigation.ts'
import { matchRoute } from '../src/lib/wm/routeMatch.ts'
import { snapshotSession } from '../src/lib/wm/session.ts'
import {
  isSecretFreeLocation,
  locationFor,
  PASSWORDS_LOCATIONS,
  passwordsRoutePatterns,
} from '../src/lib/passwords/registry.ts'
import { ALPHA, APPS, emptyState } from './lib/wm-fixtures.ts'

const MARKER = 'SECRET-MARKER-ROUTE'

const SAMPLE_PATHS: Record<string, string> = {
  list: '/',
  folder: '/folder/3f2a-9c',
  entry: '/entry/0b1e5c7a-1',
  history: '/entry/0b1e5c7a-1/history',
  trash: '/trash',
  generator: '/generator',
  import: '/import',
}

test('every place resolves through matchRoute to its own location', () => {
  for (const location of PASSWORDS_LOCATIONS) {
    const path = SAMPLE_PATHS[location.id]
    assert.ok(path, `a sample path exists for ${location.id}`)
    const match = matchRoute(passwordsRoutePatterns(), path)
    assert.ok(match, `${path} matches`)
    assert.equal(locationFor(path)?.location.id, location.id)
  }
})

test('a path that is no place does not resolve', () => {
  assert.equal(locationFor('/entry'), undefined)
  assert.equal(locationFor('/entry/a/b'), undefined)
  assert.equal(locationFor('/nothing'), undefined)
})

test('params of the places come back as given', () => {
  assert.deepEqual(locationFor('/entry/0b1e5c7a-1')?.params, {
    id: '0b1e5c7a-1',
  })
  assert.deepEqual(locationFor('/entry/x1/history')?.params, { id: 'x1' })
})

test('every place has a fixed title key under wm.passwords', () => {
  for (const location of PASSWORDS_LOCATIONS) {
    assert.match(location.titleKey, /^wm\.passwords\.[a-z]+$/)
  }
})

test('opaque ids and the allowed query keys are secret free', () => {
  for (const path of Object.values(SAMPLE_PATHS)) {
    assert.ok(isSecretFreeLocation({ path, query: {} }), path)
  }
  assert.ok(isSecretFreeLocation({ path: '/', query: { q: 'github' } }))
  assert.ok(isSecretFreeLocation({ path: '/', query: { tag: 'a1-b2' } }))
  assert.ok(isSecretFreeLocation({ path: '/entry/new', query: {} }))
  assert.ok(isSecretFreeLocation({ path: '/entry/e1', query: { edit: '' } }))
})

test('a title-like param, an unknown query key or a bad value is refused', () => {
  assert.equal(
    isSecretFreeLocation({ path: '/entry/My Bank Login', query: {} }),
    false,
  )
  assert.equal(
    isSecretFreeLocation({ path: '/entry/a b', query: {} }),
    false,
    'a value containing spaces in a param',
  )
  assert.equal(
    isSecretFreeLocation({ path: '/', query: { password: 'x' } }),
    false,
  )
  assert.equal(
    isSecretFreeLocation({ path: '/', query: { q: 'a', title: 'b' } }),
    false,
  )
  assert.equal(
    isSecretFreeLocation({ path: '/', query: { tag: 'two words' } }),
    false,
  )
  assert.equal(
    isSecretFreeLocation({ path: '/entry/e1', query: { edit: 'secret' } }),
    false,
  )
  assert.equal(isSecretFreeLocation({ path: '/unknown', query: {} }), false)
  assert.equal(
    isSecretFreeLocation({ path: '/', query: { q: 'x'.repeat(201) } }),
    false,
  )
})

test('a location survives the formatted text round trip and stays secret free', () => {
  const location = { path: '/', query: { q: 'git', tag: 't-1' } }
  const parsed = parseLocation(formatLocation(location))
  assert.deepEqual(parsed, location)
  assert.ok(isSecretFreeLocation(parsed))
})

test('a tab history of all places goes through the saved session without a value', () => {
  const state = emptyState()
  openApp(state, ALPHA.id, APPS)
  const tab = state.windows[0]!.tabs[0]!
  let history: TabHistory = createHistory('/')
  for (const path of Object.values(SAMPLE_PATHS)) {
    // Titles are the static place titles, never an entry value.
    const title = locationFor(path)?.location.titleKey ?? null
    history = push(history, path, title)
  }
  for (const entry of history.entries) {
    assert.ok(isSecretFreeLocation(entry.location), entry.location.path)
  }
  const session = snapshotSession(state, (id) =>
    id === tab.id ? history : undefined,
  )
  const text = JSON.stringify(session)
  assert.equal(text.includes(MARKER), false)
  assert.ok(text.includes('/entry/0b1e5c7a-1/history'))
})

test('a place that would carry an entry title is refused before it reaches a history', () => {
  const leaking: { path: string; query: Record<string, string> }[] = [
    { path: `/entry/${MARKER} login`, query: {} },
    { path: '/', query: { title: MARKER } },
    { path: `/folder/${MARKER}/x`, query: {} },
  ]
  for (const location of leaking) {
    assert.equal(isSecretFreeLocation(location), false, location.path)
  }
})

// The locale files stay in lockstep for everything the password manager introduces (T098) and for the
// fields and the appearance (spec 035, T045): every key under these prefixes exists in German and
// English, and no value is empty.
const LOCALE_PREFIXES = [
  'passwords',
  'wm.passwords',
  'actions.passwords',
  'errors.passwords',
  'fields',
  'settings.appearance',
  'actions.settings.appearance',
]

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

test('the German and English texts of the password manager have the same keys', () => {
  const de = locale('de')
  const en = locale('en')
  const owned = (key: string) =>
    LOCALE_PREFIXES.some(
      (prefix) => key === prefix || key.startsWith(`${prefix}.`),
    )
  const deKeys = [...de.keys()].filter(owned).sort()
  const enKeys = [...en.keys()].filter(owned).sort()
  assert.ok(
    deKeys.length > 100,
    'the password manager, fields and appearance have their texts',
  )
  assert.deepEqual(
    deKeys.filter((key) => !en.has(key)),
    [],
    'keys that only German has',
  )
  assert.deepEqual(
    enKeys.filter((key) => !de.has(key)),
    [],
    'keys that only English has',
  )
  for (const key of deKeys) {
    assert.notEqual(de.get(key)?.trim(), '', `empty German text ${key}`)
    assert.notEqual(en.get(key)?.trim(), '', `empty English text ${key}`)
  }
})

test('the new entry may name the type pass and the entry it belongs to, nothing else may', () => {
  assert.ok(
    isSecretFreeLocation({ path: '/entry/new', query: { type: 'pass' } }),
  )
  assert.ok(
    isSecretFreeLocation({
      path: '/entry/new',
      query: { type: 'pass', parent: 'a1-b2' },
    }),
  )
  assert.ok(
    isSecretFreeLocation({ path: '/entry/new', query: { parent: 'a1-b2' } }),
  )
  // Only the value `pass`, only an opaque id, only on the new entry.
  assert.equal(
    isSecretFreeLocation({ path: '/entry/new', query: { type: 'login' } }),
    false,
  )
  assert.equal(
    isSecretFreeLocation({ path: '/entry/new', query: { type: 'Concert' } }),
    false,
  )
  assert.equal(
    isSecretFreeLocation({
      path: '/entry/new',
      query: { parent: 'My Bank Login' },
    }),
    false,
  )
  assert.equal(
    isSecretFreeLocation({ path: '/entry/e1', query: { type: 'pass' } }),
    false,
  )
  assert.equal(
    isSecretFreeLocation({ path: '/entry/e1', query: { parent: 'a1' } }),
    false,
  )
  assert.equal(
    isSecretFreeLocation({ path: '/', query: { type: 'pass' } }),
    false,
  )
})
