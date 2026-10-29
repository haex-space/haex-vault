// Part of `pnpm check:extensions` (spec 017, T025): installed extensions as apps of the window
// manager (src/lib/extensions/apps.ts) and their tabs in a restored session
// (src/lib/wm/sessionSync.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import type { ExtensionSummary } from '../src/types/bindings/ExtensionSummary.ts'
import {
  allApps,
  extensionAppId,
  extensionApps,
  extensionIdOf,
  statusErrorKey,
  tabOfExtension,
  tabsOfStoppedExtensions,
} from '../src/lib/extensions/apps.ts'
import { readConsoleForward } from '../src/lib/extensions/devConsole.ts'
import { WM_APPS, type AppDefinition } from '../src/lib/wm/apps.ts'
import { openApp } from '../src/lib/wm/layoutState.ts'
import { snapshotSession } from '../src/lib/wm/session.ts'
import {
  createSessionSync,
  type SessionPort,
} from '../src/lib/wm/sessionSync.ts'
import type { TabHistory } from '../src/lib/wm/navigation.ts'
import { emptyState } from './lib/wm-fixtures.ts'

function summary(patch: Partial<ExtensionSummary>): ExtensionSummary {
  return {
    id: '00000000-0000-0000-0000-000000000001',
    name: 'notes',
    title: 'Notizen',
    publisherFingerprint: 'aaaa',
    enabled: true,
    state: 'installed',
    singleInstance: false,
    hasIcon: false,
    devices: [],
    dev: false,
    openers: [],
    ...patch,
  }
}

test('every installed and enabled extension is an app with its own name and icon', () => {
  const notes = summary({ description: 'Schnelle Notizen' })
  const single = summary({
    id: 'id-2',
    title: 'Kalender',
    singleInstance: true,
  })
  const apps = extensionApps(
    [
      notes,
      single,
      summary({ id: 'off', enabled: false }),
      summary({ id: 'gone', state: 'removed' }),
    ],
    { [notes.id]: 'data:image/svg+xml;base64,AA==' },
  )
  assert.deepEqual(
    apps.map((a) => a.id),
    [extensionAppId(notes.id), 'extension.id-2'],
  )
  assert.equal(apps[0]?.title, 'Notizen')
  // The agent describes an extension with it instead of its id (spec 046, R17).
  assert.equal(apps[0]?.description, 'Schnelle Notizen')
  assert.equal(apps[1]?.description, undefined)
  assert.equal(apps[0]?.iconUrl, 'data:image/svg+xml;base64,AA==')
  assert.equal(apps[0]?.multiInstance, true)
  assert.equal(apps[1]?.iconUrl, undefined)
  assert.equal(apps[1]?.multiInstance, false)
  assert.equal(apps[1]?.tabTitle, 'app')
  assert.deepEqual(allApps(apps).slice(0, WM_APPS.length), [...WM_APPS])
  assert.equal(extensionIdOf('extension.abc'), 'abc')
  assert.equal(extensionIdOf('system.chat'), null)
})

test('an extension that cannot open here is a disabled entry with the reason', () => {
  const apps = extensionApps(
    [
      summary({ id: 'new' }),
      summary({ id: 'ok', statusHere: 'ready' }),
      summary({ id: 'moving', statusHere: 'transferring' }),
      summary({ id: 'broken', statusHere: 'signature_failed' }),
      summary({ id: 'stuck', statusHere: 'migration_failed' }),
    ],
    {},
  )
  assert.deepEqual(
    apps.map((a) => a.unavailableKey ?? null),
    [
      null,
      null,
      'extensions.status.transferring',
      'extensions.status.signature_failed',
      'extensions.status.migration_failed',
    ],
  )
})

test('the error of a state names the broken rule or what went wrong', () => {
  assert.equal(
    statusErrorKey('signature_failed', 'file_mismatch'),
    'extensions.install.errors.file_mismatch',
  )
  assert.equal(
    statusErrorKey('migration_failed', 'migration_missing'),
    'extensions.statusError.migration_missing',
  )
  assert.equal(statusErrorKey('ready', undefined), null)
})

function savedSessionWith(app: AppDefinition) {
  const state = emptyState()
  const histories = new Map<string, TabHistory>()
  openApp(state, app.id, [app])
  return snapshotSession(state, (tabId) => histories.get(tabId))
}

function restoring(
  session: ReturnType<typeof savedSessionWith>,
  apps: () => readonly AppDefinition[],
) {
  const state = emptyState()
  const port: SessionPort = {
    saveNow: () => {},
    saveSoon: () => {},
    load: async () => ({ restore: { enabled: true }, session }),
    setRestore: async (enabled) => ({ enabled }),
    getRestore: async () => ({ enabled: true }),
    flushAsync: async () => {},
  }
  const sync = createSessionSync({
    state,
    histories: new Map(),
    apps,
    port,
    onRestored: () => {},
  })
  return { state, sync }
}

test('an extension tab survives the restore when the list is loaded by then', async () => {
  const [notes] = extensionApps([summary({})], {})
  assert.ok(notes)
  const session = savedSessionWith(notes)

  let loaded: AppDefinition[] = []
  const { state, sync } = restoring(session, () => allApps(loaded))
  // The page loads the list first, then restores: the getter is read at restore time.
  loaded = extensionApps([summary({})], {})
  await sync.restoreAsync()
  assert.deepEqual(
    state.windows.flatMap((w) => w.tabs.map((t) => t.appId)),
    [notes.id],
  )
})

test('a tab of an extension that is unknown after loading the list is dropped', async () => {
  const [notes] = extensionApps([summary({})], {})
  assert.ok(notes)
  const { state, sync } = restoring(savedSessionWith(notes), () => allApps([]))
  await sync.restoreAsync()
  assert.equal(state.windows.length, 0)
})

test('the tabs of a disabled or removed extension close, those of one not ready here stay', () => {
  const [running, disabled, removed, moving] = [1, 2, 3, 4].map(
    (n) => `00000000-0000-0000-0000-00000000000${n}`,
  ) as [string, string, string, string]
  const ids = [running, disabled, removed, moving]
  // Opened while all four ran.
  const before = extensionApps(
    ids.map((id) => summary({ id })),
    {},
  )
  const state = emptyState()
  for (const id of ids) openApp(state, extensionAppId(id), allApps(before))
  openApp(state, 'system.chat', allApps(before))

  const now = [
    summary({ id: running }),
    summary({ id: disabled, enabled: false }),
    summary({ id: removed, state: 'removed' }),
    summary({ id: moving, statusHere: 'transferring' }),
  ]
  const appOf = new Map(
    state.windows.flatMap((w) => w.tabs).map((t) => [t.id, t.appId]),
  )
  assert.deepEqual(
    tabsOfStoppedExtensions(state.windows, now).map((c) =>
      extensionIdOf(appOf.get(c.tabId) ?? ''),
    ),
    [disabled, removed],
  )
})

test('a notification click finds the first tab of its extension, or none', () => {
  const [notes, calendar] = [1, 2].map(
    (n) => `00000000-0000-0000-0000-00000000000${n}`,
  ) as [string, string]
  const apps = allApps(extensionApps([summary({ id: notes })], {}))
  const state = emptyState()
  openApp(state, 'system.chat', apps)
  openApp(state, extensionAppId(notes), apps)
  const found = tabOfExtension(state.windows, notes)
  const tab = state.windows
    .flatMap((w) =>
      w.tabs.map((t) => ({ windowId: w.id, tabId: t.id, appId: t.appId })),
    )
    .find((t) => t.appId === extensionAppId(notes))
  assert.deepEqual(found, tab && { windowId: tab.windowId, tabId: tab.tabId })
  assert.equal(tabOfExtension(state.windows, calendar), null)
})

test('a development version is an app with the dev mark', () => {
  const [draft, signed] = extensionApps(
    [summary({ id: 'draft', dev: true }), summary({ id: 'signed' })],
    {},
  )
  assert.equal(draft?.dev, true)
  assert.equal(signed?.dev, undefined)
})

test('only a console.forward message of the SDK is a console line', () => {
  assert.deepEqual(
    readConsoleForward({
      type: 'console.forward',
      data: { level: 'warn', message: 'careful', timestamp: '12:00:00' },
    }),
    { level: 'warn', message: 'careful', time: '12:00:00' },
  )
  assert.equal(
    readConsoleForward({
      type: 'console.forward',
      data: { level: 'trace', message: 'x' },
    }),
    null,
  )
  assert.equal(readConsoleForward({ type: 'haexspace:port:ready' }), null)
  assert.equal(readConsoleForward('console.forward'), null)
})
