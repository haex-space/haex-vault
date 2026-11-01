// `pnpm check:settings` (spec 023-settings-app): the settings registry, header back, color
// scheme and the alias of the removed federation app (research R11). Pure modules, loaded with
// Node type-stripping like check-wm-state.ts.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

import {
  categoryOf,
  headerBack,
  locationFor,
  overviewGroups,
  overviewRows,
  SETTINGS_CATEGORIES,
  SETTINGS_LOCATIONS,
  settingsRoutePatterns,
} from '../src/lib/settings/registry.ts'
import { isDark, parseColorScheme } from '../src/lib/settings/colorScheme.ts'
import { parseLanguage } from '../src/lib/settings/language.ts'
import { isBackgroundValue } from '../src/lib/settings/background.ts'
import {
  canManageDevices,
  deviceStatus,
  elapsedSince,
  problemLabelKey,
  problemMessageKey,
  roleLabelKey,
  schemaDiff,
  sortDevices,
} from '../src/lib/sync/deviceStatus.ts'
import {
  serverEntries,
  withEnabled,
  withoutServer,
} from '../src/lib/sync/serverEntries.ts'
import { searchSettings } from '../src/lib/settings/search.ts'
import {
  getAppDefinition,
  resolveAppAlias,
  tabTitleFor,
  WM_APPS,
} from '../src/lib/wm/apps.ts'
import { createHistory, push } from '../src/lib/wm/navigation.ts'
import { matchRoute } from '../src/lib/wm/routeMatch.ts'

const LOCATION_PATHS: Record<string, string> = {
  general: '/',
  'general.basic': '/general/basic',
  'general.basic.password': '/general/basic/password',
  'general.appearance': '/general/appearance',
  'general.dock': '/general/dock',
  'general.syncLog': '/general/sync-log',
  'general.licenses': '/general/licenses',
  ai: '/ai',
  'ai.agents': '/ai/agents',
  'ai.agents.agent': '/ai/agents/6d637021-4a3e-4b8c-9f12-0e7d5a21c021',
  'ai.agents.log': '/ai/agents/6d637021-4a3e-4b8c-9f12-0e7d5a21c021/log',
  'ai.browser': '/ai/browser',
  'ai.default': '/ai/default',
  'ai.providers': '/ai/providers',
  'ai.servers': '/ai/servers',
  'ai.servers.server': '/ai/servers/0b9f1c2e-5d6a-4e7b-8c9d-0a1b2c3d4e5f',
  'ai.installed': '/ai/installed',
  'ai.download': '/ai/download',
  'ai.download.search': '/ai/download/search',
  'ai.download.repo': '/ai/download/repo/Qwen/Qwen2.5-GGUF',
  'ai.speech': '/ai/speech',
  'ai.speech.voice': '/ai/speech/voice',
  'ai.speech.tones': '/ai/speech/tones',
  'ai.autonomy': '/ai/autonomy',
  'ai.denyRules': '/ai/deny-rules',
  'ai.files': '/ai/files',
  storage: '/storage',
  'storage.connection': '/storage/connections/new',
  'storage.storage': '/storage/connections/c1/storages/new',
  'storage.remove': '/storage/remove/connection/c1',
  contacts: '/contacts',
  extensions: '/extensions',
  'extensions.browserBridge': '/extensions/browser-bridge',
  'extensions.marketplaces': '/extensions/marketplaces',
  'extensions.marketplaces.edit': '/extensions/marketplaces/new',
  'extensions.detail': '/extensions/00000000-0000-0000-0000-000000000001',
  federation: '/federation',
  'federation.link': '/federation/devices/link',
  'federation.remove': '/federation/devices/abc123/remove',
}

function flatKeys(tree: unknown, prefix = ''): Set<string> {
  const keys = new Set<string>()
  if (typeof tree !== 'object' || tree === null) return keys
  for (const [key, value] of Object.entries(tree)) {
    const path = prefix ? `${prefix}.${key}` : key
    if (typeof value === 'string') keys.add(path)
    else for (const nested of flatKeys(value, path)) keys.add(nested)
  }
  return keys
}

function localeTree(locale: 'de' | 'en'): unknown {
  const url = new URL(`../src/i18n/locales/${locale}.json`, import.meta.url)
  return JSON.parse(readFileSync(url, 'utf8'))
}

function localeKeys(locale: 'de' | 'en'): Set<string> {
  return flatKeys(localeTree(locale))
}

/** A plain `t` over the German locale, without vue-i18n: params stay as `{name}`. */
function germanT(): (key: string) => string {
  const tree = localeTree('de')
  return (key) => {
    let node: unknown = tree
    for (const part of key.split('.')) {
      node = (node as Record<string, unknown> | undefined)?.[part]
    }
    if (typeof node !== 'string') throw new Error(`missing de key ${key}`)
    return node
  }
}

test('six categories in the order of FR-005 (storage from spec 038, contacts from spec 051, extensions from spec 017, no appearance since spec 042, models and agents merged into ai), each at its own location', () => {
  assert.deepEqual(
    SETTINGS_CATEGORIES.map((category) => [category.id, category.path]),
    [
      ['general', '/'],
      ['ai', '/ai'],
      ['storage', '/storage'],
      ['contacts', '/contacts'],
      ['extensions', '/extensions'],
      ['federation', '/federation'],
    ],
  )
  for (const category of SETTINGS_CATEGORIES) {
    assert.ok(category.icon.startsWith('lucide:'), category.id)
    assert.equal(locationFor(category.path)?.location.id, category.id)
  }
})

test('locations have unique ids and patterns, a category, and titles', () => {
  const ids = SETTINGS_LOCATIONS.map((location) => location.id)
  const patterns = SETTINGS_LOCATIONS.map((location) => location.pattern)
  assert.equal(new Set(ids).size, ids.length)
  assert.equal(new Set(patterns).size, patterns.length)
  assert.deepEqual([...ids].sort(), Object.keys(LOCATION_PATHS).sort())
  const categoryIds = new Set(
    SETTINGS_CATEGORIES.map((category) => category.id),
  )
  for (const location of SETTINGS_LOCATIONS) {
    assert.ok(categoryIds.has(location.category), location.id)
    assert.ok(location.titleKey.length > 0, location.id)
    if (location.parent) {
      const parent = SETTINGS_LOCATIONS.find(
        (candidate) => candidate.id === location.parent,
      )
      assert.ok(parent, `${location.id} → ${location.parent}`)
      assert.equal(parent.category, location.category, location.id)
    }
    if (location.overviewRow) {
      assert.ok(location.icon?.startsWith('lucide:'), location.id)
      assert.ok(location.descriptionKey, location.id)
    }
  }
})

test('overview rows of general (spec 042) and ai; federation has none', () => {
  const rows = (id: Parameters<typeof overviewRows>[0]) =>
    overviewRows(id).map((location) => location.id)
  assert.deepEqual(rows('ai'), [
    'ai.default',
    'ai.providers',
    'ai.servers',
    'ai.installed',
    'ai.download',
    'ai.speech',
    'ai.speech.voice',
    'ai.speech.tones',
    'ai.autonomy',
    'ai.denyRules',
    'ai.files',
    'ai.agents',
    'ai.browser',
  ])
  assert.deepEqual(rows('general'), [
    'general.basic',
    'general.appearance',
    'general.dock',
    'general.syncLog',
    'general.licenses',
  ])
  assert.deepEqual(rows('federation'), [])
})

test('the ai overview groups its rows by task, one heading each', () => {
  assert.deepEqual(
    overviewGroups('ai').map((g) => [g.group, g.rows.map((row) => row.id)]),
    [
      ['chat', ['ai.default', 'ai.providers', 'ai.servers']],
      ['local', ['ai.installed', 'ai.download']],
      ['speech', ['ai.speech', 'ai.speech.voice', 'ai.speech.tones']],
      [
        'agents',
        ['ai.autonomy', 'ai.denyRules', 'ai.files', 'ai.agents', 'ai.browser'],
      ],
    ],
  )
  // Rows without a group stay one list each, in order.
  assert.deepEqual(
    overviewGroups('general').map((g) => [g.group, g.rows.length]),
    [
      [undefined, 1],
      [undefined, 1],
      [undefined, 1],
      [undefined, 1],
      [undefined, 1],
    ],
  )
})

test('the former models and agents paths no longer resolve', () => {
  for (const path of [
    '/models',
    '/models/installed',
    '/agents',
    '/agents/files',
  ]) {
    assert.equal(locationFor(path), undefined, path)
  }
})

test('the route patterns resolve every location, with the repo params', () => {
  const routes = settingsRoutePatterns()
  for (const [id, path] of Object.entries(LOCATION_PATHS)) {
    const match = matchRoute(routes, path)
    assert.ok(match, path)
    assert.equal(match.chain.length, 2, path)
    assert.equal(locationFor(path)?.location.id, id, path)
  }
  assert.deepEqual(locationFor('/ai/download/repo/Qwen/Qwen2.5-GGUF')?.params, {
    owner: 'Qwen',
    name: 'Qwen2.5-GGUF',
  })
  assert.equal(locationFor('/nope'), undefined)
  assert.equal(locationFor('/ai/nope'), undefined)
})

test('categoryOf maps every location to its category', () => {
  assert.equal(categoryOf('/'), 'general')
  assert.equal(categoryOf('/ai/download/repo/Qwen/Qwen2.5-GGUF'), 'ai')
  assert.equal(categoryOf('/ai/deny-rules'), 'ai')
  assert.equal(categoryOf('/federation'), 'federation')
  assert.equal(categoryOf('/nope'), undefined)
})

test('every registry text exists in German and English (FR-020)', () => {
  const keys = [
    ...SETTINGS_CATEGORIES.map((category) => category.titleKey),
    ...SETTINGS_LOCATIONS.flatMap((location) =>
      location.descriptionKey
        ? [location.titleKey, location.descriptionKey]
        : [location.titleKey],
    ),
    ...SETTINGS_LOCATIONS.flatMap((location) => [
      ...(location.keywordsKey ? [location.keywordsKey] : []),
      ...(location.settingKeys ?? []),
    ]),
    ...(['chat', 'local', 'speech', 'agents'] as const).map(
      (group) => `settings.groups.${group}`,
    ),
    'settings.back',
    'settings.search.label',
    'settings.search.placeholder',
    'settings.search.noResults',
    'settings.search.open',
    'settings.search.clear',
    'settings.search.close',
    'settings.sidebar.show',
    'settings.sidebar.hide',
  ]
  for (const locale of ['de', 'en'] as const) {
    const available = localeKeys(locale)
    const missing = keys.filter((key) => !available.has(key))
    assert.deepEqual(missing, [], locale)
  }
})

test('header back returns to the previous station in the same category', () => {
  const fromSearch = push(
    push(createHistory('/ai/download'), '/ai/download/search?q=qwen'),
    '/ai/download/repo/Qwen/Qwen2.5-GGUF',
  )
  assert.deepEqual(headerBack(fromSearch), {
    kind: 'back',
    path: '/ai/download/search',
  })
  const fromOverview = push(createHistory('/ai'), '/ai/installed')
  assert.deepEqual(headerBack(fromOverview), { kind: 'back', path: '/ai' })
  // Modelle → Installierte Modelle → Modelle herunterladen: back to Installierte Modelle.
  const lateral = push(
    push(createHistory('/ai'), '/ai/installed'),
    '/ai/download',
  )
  assert.deepEqual(headerBack(lateral), {
    kind: 'back',
    path: '/ai/installed',
  })
})

test('header back goes to the parent after a jump from another category or a deep link', () => {
  const fromElsewhere = push(createHistory('/storage'), '/ai/installed')
  assert.deepEqual(headerBack(fromElsewhere), {
    kind: 'push',
    path: '/ai',
  })
  const deepLink = createHistory('/ai/download/repo/Qwen/Qwen2.5-GGUF')
  assert.deepEqual(headerBack(deepLink), {
    kind: 'push',
    path: '/ai/download/search',
  })
  assert.equal(headerBack(createHistory('/ai')), undefined)
})

test('the settings tab keeps the app title; other apps show the location title', () => {
  const routed = {
    key: 'settings.locations.ai.installed.title',
    params: {},
  }
  const settings = getAppDefinition('system.settings')
  assert.equal(settings?.tabTitle, 'app')
  assert.deepEqual(tabTitleFor(settings, routed), {
    key: 'wm.apps.settings',
    params: {},
  })
  const thread = { key: 'wm.chat.thread', params: { id: 't1' } }
  assert.deepEqual(tabTitleFor(getAppDefinition('system.chat'), thread), thread)
  assert.deepEqual(tabTitleFor(undefined, thread), thread)
})

test('the removed federation app opens the settings at the federation category (R11)', () => {
  assert.deepEqual(resolveAppAlias('system.federation'), {
    appId: 'system.settings',
    at: '/federation',
  })
  assert.deepEqual(resolveAppAlias('system.chat'), {
    appId: 'system.chat',
    at: null,
  })
  assert.ok(!WM_APPS.some((app) => app.id === 'system.federation'))
})

test('every location without route params is searchable', () => {
  for (const location of SETTINGS_LOCATIONS) {
    assert.equal(
      location.keywordsKey !== undefined,
      !location.pattern.includes(':'),
      location.id,
    )
  }
})

test('the search finds locations by title, synonym and single setting (FR-023)', () => {
  const t = germanT()
  const first = (query: string) => searchSettings(query, t)[0]
  assert.deepEqual(searchSettings('   ', t), [])
  assert.deepEqual(searchSettings('xyzzy', t), [])

  assert.equal(first('standard')?.path, '/ai/default')
  assert.equal(first('whisper')?.path, '/ai/speech')
  assert.equal(first('dunkel')?.path, '/general/appearance')

  const alias = first('geratename')
  assert.equal(alias?.label, 'Gerätename')
  assert.equal(alias?.path, '/general/basic')
  assert.deepEqual(alias?.trail, ['Allgemein', 'Grundeinstellung'])

  const search = first('huggingface suchen')
  assert.equal(search?.path, '/ai/download/search')
  assert.deepEqual(search?.trail, ['KI', 'Modelle herunterladen'])

  assert.ok(
    searchSettings('modell', t).every((hit) => !hit.path.includes('/repo/')),
  )
})

test('a title match ranks above a synonym match', () => {
  const t = germanT()
  const labels = searchSettings('sitzung', t).map((hit) => hit.label)
  assert.deepEqual(labels, [
    t('settings.sessionRestore.title'),
    'Grundeinstellung',
  ])
  assert.equal(searchSettings('modelle', t)[0]?.path, '/ai/download')
})

test('color scheme: known values parse, anything else is unset', () => {
  assert.equal(parseColorScheme('light'), 'light')
  assert.equal(parseColorScheme('dark'), 'dark')
  assert.equal(parseColorScheme('system'), 'system')
  for (const value of ['Dark', '', null, undefined, 1]) {
    assert.equal(parseColorScheme(value), null, String(value))
  }
})

test('language (spec 042): de and en parse, anything else is unset', () => {
  assert.equal(parseLanguage('de'), 'de')
  assert.equal(parseLanguage('en'), 'en')
  for (const value of ['DE', 'fr', 'de-DE', '', null, undefined, 1]) {
    assert.equal(parseLanguage(value), null, String(value))
  }
})

test('background (spec 042): only a WebP data URL is shown', () => {
  assert.equal(isBackgroundValue('data:image/webp;base64,UklGRg=='), true)
  for (const value of [
    'data:image/png;base64,iVBORw0KGgo=',
    'data:image/webp;base64,',
    'data:image/webp;base64,UklGRg==") ; color: red',
    'https://example.com/a.webp',
    'url(x)',
    '',
    null,
    undefined,
  ]) {
    assert.equal(isBackgroundValue(value), false, String(value))
  }
})

test('color scheme: dark follows the system only for system', () => {
  assert.equal(isDark('system', true), true)
  assert.equal(isDark('system', false), false)
  assert.equal(isDark('light', true), false)
  assert.equal(isDark('dark', false), true)
})

test('device status: online, never seen, or the last time (FR-033)', () => {
  const base = { alias: 'A', isCurrent: false }
  assert.deepEqual(deviceStatus({ ...base, online: true, lastSeen: 5 }), {
    kind: 'online',
  })
  assert.deepEqual(deviceStatus({ ...base, online: false, lastSeen: null }), {
    kind: 'neverSeen',
  })
  assert.deepEqual(deviceStatus({ ...base, online: false, lastSeen: 1234 }), {
    kind: 'lastSeen',
    at: 1234,
  })
})

test('elapsed time reads in the largest whole unit, and the future as just now', () => {
  const now = 1_000_000_000
  assert.deepEqual(elapsedSince(now - 30_000, now), { unit: 'justNow' })
  assert.deepEqual(elapsedSince(now - 5 * 60_000, now), {
    unit: 'minutes',
    value: 5,
  })
  assert.deepEqual(elapsedSince(now - 59 * 60_000, now), {
    unit: 'minutes',
    value: 59,
  })
  assert.deepEqual(elapsedSince(now - 3 * 3_600_000 - 1000, now), {
    unit: 'hours',
    value: 3,
  })
  assert.deepEqual(elapsedSince(now - 2 * 86_400_000, now), {
    unit: 'days',
    value: 2,
  })
  assert.deepEqual(elapsedSince(now + 99_000, now), { unit: 'justNow' })
})

test('devices sort this one first, then by name ignoring case, unnamed last', () => {
  const device = (alias: string | null, isCurrent = false) => ({
    alias,
    isCurrent,
    online: false,
    lastSeen: null,
  })
  const sorted = sortDevices([
    device(null),
    device('beta'),
    device('Zeta', true),
    device('Alpha'),
    device('  '),
  ])
  assert.deepEqual(
    sorted.map((d) => d.alias),
    ['Zeta', 'Alpha', 'beta', null, '  '],
  )
})

test('roles and problems name texts that exist in German and English (FR-037)', () => {
  const keys = [
    roleLabelKey('main'),
    roleLabelKey('linked'),
    problemLabelKey('incompatible_version'),
    problemLabelKey('duplicate'),
    problemMessageKey('incompatible_version', {
      protocol: 'equal',
      holziMigration: 'lower',
      crdtTrigger: 'equal',
    }),
    problemMessageKey('incompatible_version', {
      protocol: 'equal',
      holziMigration: 'higher',
      crdtTrigger: 'equal',
    }),
  ]
  for (const locale of ['de', 'en'] as const) {
    const available = localeKeys(locale)
    assert.deepEqual(
      keys.filter((key) => !available.has(key)),
      [],
      locale,
    )
  }
})

const OWN = { protocol: 2, holziMigration: 40, crdtTrigger: 27 }

test('schema diff: each value of the other device against this one, unknown while either is missing (spec 059)', () => {
  assert.deepEqual(schemaDiff(OWN, { ...OWN }), {
    protocol: 'equal',
    holziMigration: 'equal',
    crdtTrigger: 'equal',
  })
  assert.deepEqual(
    schemaDiff(OWN, { protocol: 2, holziMigration: 39, crdtTrigger: 28 }),
    { protocol: 'equal', holziMigration: 'lower', crdtTrigger: 'higher' },
  )
  assert.equal(schemaDiff(OWN, null), null)
  assert.equal(schemaDiff(null, OWN), null)
})

test('the version message names the device to update only when every difference points one way (FR-012)', () => {
  const key = (other: typeof OWN | null) =>
    problemMessageKey('incompatible_version', schemaDiff(OWN, other))
  const plain = 'settings.federation.problem.incompatible_version'
  assert.equal(key({ ...OWN, holziMigration: 39 }), `${plain}_other`)
  assert.equal(
    key({ protocol: 1, holziMigration: 39, crdtTrigger: 27 }),
    `${plain}_other`,
  )
  assert.equal(key({ ...OWN, holziMigration: 41 }), `${plain}_this`)
  // Opposite directions say nothing about who has to update.
  assert.equal(key({ ...OWN, holziMigration: 39, crdtTrigger: 28 }), plain)
  // Unknown state, or no difference at all: the text of before.
  assert.equal(key(null), plain)
  assert.equal(key({ ...OWN }), plain)
  // A duplicate keeps its own text whatever the values say.
  assert.equal(
    problemMessageKey('duplicate', schemaDiff(OWN, { ...OWN, protocol: 1 })),
    'settings.federation.problem.duplicate',
  )
})

test('only a main device manages devices (FR-035)', () => {
  assert.equal(canManageDevices('main'), true)
  for (const other of [
    'linked',
    'awaiting_admission',
    'removed',
    null,
  ] as const) {
    assert.equal(canManageDevices(other), false, String(other))
  }
})

test('server entries list the built-in servers first, then the added ones, with the switched off ones kept', () => {
  assert.deepEqual(
    serverEntries(['wss://a', 'wss://b'], ['wss://c', 'wss://a'], ['wss://b']),
    [
      { url: 'wss://a', enabled: true, isDefault: true },
      { url: 'wss://b', enabled: false, isDefault: true },
      { url: 'wss://c', enabled: true, isDefault: false },
    ],
  )
})

test('switching a server off remembers it once, switching it on forgets it', () => {
  assert.deepEqual(withEnabled([], 'wss://a', false), ['wss://a'])
  assert.deepEqual(withEnabled(['wss://a'], 'wss://a', false), ['wss://a'])
  assert.deepEqual(withEnabled(['wss://a', 'wss://b'], 'wss://a', true), [
    'wss://b',
  ])
})

test('removing an added server forgets that it was switched off', () => {
  assert.deepEqual(
    withoutServer(['wss://c', 'wss://d'], ['wss://c', 'wss://b'], 'wss://c'),
    { added: ['wss://d'], disabled: ['wss://b'] },
  )
})
