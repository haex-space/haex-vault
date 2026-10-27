// Part of `pnpm check:wm-navigation` (spec 020-tab-navigation): store-level navigation — tab
// histories kept in sync with the layout (tabNavigation.ts) and the handler registries the action
// runner uses (actions/handlers.ts). Pure modules, no Pinia/Nuxt; the store only wires them.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  createHandlerRegistry,
  type ActionHandlerContext,
} from '../src/lib/actions/handlers.ts'
import {
  closeWindow,
  hydrate,
  minimizeWindow,
  moveWindowToWorkspace,
  switchWorkspace,
  toggleMaximizeWindow,
  createWorkspace,
} from '../src/lib/wm/layoutState.ts'
import {
  currentLocation,
  createHistory,
  MAX_HISTORY_ENTRIES,
  push,
  type TabHistory,
} from '../src/lib/wm/navigation.ts'
import {
  addTabAt,
  goTab,
  navigateTab,
  openAppAt,
  resolveSystemBack,
  setTabHistory,
  skipTabEntry,
  syncHistories,
} from '../src/lib/wm/tabNavigation.ts'
import { closeTab, switchTab } from '../src/lib/wm/tabs.ts'
import type { WmState } from '../src/lib/wm/types.ts'
import { ALPHA, APPS, AREA, BETA, emptyState } from './lib/wm-fixtures.ts'

type Histories = Map<string, TabHistory>

function pathOf(histories: Histories, tabId: string | null): string | null {
  const history = tabId ? histories.get(tabId) : undefined
  return history ? currentLocation(history).path : null
}

function fresh(): { state: WmState; histories: Histories } {
  return { state: emptyState(), histories: new Map() }
}

// ---------------------------------------------------------------------------
// Tab histories (tabNavigation.ts)
// ---------------------------------------------------------------------------

test('a newly opened tab starts with one `/` entry', () => {
  const { state, histories } = fresh()
  const { tabId, created } = openAppAt(state, histories, ALPHA.id, null, APPS)
  assert.equal(created, true)
  assert.equal(histories.get(tabId ?? '')?.entries.length, 1)
  assert.equal(pathOf(histories, tabId), '/')
})

test('openAppAt on a closed app starts a single-entry history at the location', () => {
  const { state, histories } = fresh()
  const { tabId } = openAppAt(state, histories, ALPHA.id, '/models', APPS)
  assert.equal(pathOf(histories, tabId), '/models')
  assert.equal(histories.get(tabId ?? '')?.entries.length, 1)
})

test('openAppAt on an open singleton activates it and pushes the location', () => {
  const { state, histories } = fresh()
  const first = openAppAt(state, histories, ALPHA.id, null, APPS)
  openAppAt(state, histories, BETA.id, null, APPS)
  const again = openAppAt(state, histories, ALPHA.id, '/models', APPS)
  assert.equal(again.created, false)
  assert.equal(again.tabId, first.tabId)
  assert.equal(state.windows.length, 2)
  const history = histories.get(first.tabId ?? '')
  assert.equal(history?.entries.length, 2)
  assert.equal(pathOf(histories, first.tabId), '/models')
})

test('openAppAt with the current location of an open singleton adds no entry', () => {
  const { state, histories } = fresh()
  const first = openAppAt(state, histories, ALPHA.id, '/models', APPS)
  openAppAt(state, histories, ALPHA.id, '/models', APPS)
  assert.equal(histories.get(first.tabId ?? '')?.entries.length, 1)
})

test('addTabAt adds a tab at the location, or navigates an open singleton', () => {
  const { state, histories } = fresh()
  const alpha = openAppAt(state, histories, ALPHA.id, null, APPS)
  const windowId = state.windows[0]?.id ?? ''
  const beta = addTabAt(state, histories, windowId, BETA.id, '/x', APPS)
  assert.equal(beta.created, true)
  assert.equal(pathOf(histories, beta.tabId), '/x')
  const reopened = addTabAt(state, histories, windowId, ALPHA.id, '/y', APPS)
  assert.equal(reopened.tabId, alpha.tabId)
  assert.equal(pathOf(histories, alpha.tabId), '/y')
})

test('navigating one tab never changes another (FR-008)', () => {
  const { state, histories } = fresh()
  const a = openAppAt(state, histories, ALPHA.id, null, APPS).tabId ?? ''
  const b = openAppAt(state, histories, BETA.id, null, APPS).tabId ?? ''
  const before = histories.get(b)
  navigateTab(histories, a, '/one')
  navigateTab(histories, a, '/two')
  goTab(histories, a, -1)
  assert.equal(histories.get(b), before)
  assert.equal(pathOf(histories, a), '/one')
})

test('window and workspace operations keep every history (FR-010)', () => {
  const { state, histories } = fresh()
  const a = openAppAt(state, histories, ALPHA.id, null, APPS).tabId ?? ''
  navigateTab(histories, a, '/deep')
  const windowId = state.windows[0]?.id ?? ''
  const snapshot = histories.get(a)
  const other = createWorkspace(state, 'ws-2')
  minimizeWindow(state, windowId)
  toggleMaximizeWindow(state, windowId)
  moveWindowToWorkspace(state, windowId, other.id)
  switchWorkspace(state, other.id)
  switchTab(state, windowId, a)
  syncHistories(histories, state)
  assert.equal(histories.get(a), snapshot)
})

test('closing a tab or window drops its history on the next sync (FR-011)', () => {
  const { state, histories } = fresh()
  const alpha = openAppAt(state, histories, ALPHA.id, null, APPS).tabId ?? ''
  const windowId = state.windows[0]?.id ?? ''
  const beta =
    addTabAt(state, histories, windowId, BETA.id, null, APPS).tabId ?? ''
  closeTab(state, windowId, beta)
  syncHistories(histories, state)
  assert.equal(histories.has(beta), false)
  closeWindow(state, windowId)
  syncHistories(histories, state)
  assert.equal(histories.has(alpha), false)
})

test('hydrate plus sync gives every restored tab exactly `[/]`', () => {
  const state = hydrate(
    {
      workspaces: [{ id: 'ws', position: 0 }],
      windows: [
        {
          id: 'w',
          workspaceId: 'ws',
          x: 0,
          y: 0,
          width: 800,
          height: 600,
          minimized: false,
          maximized: false,
          stack: 1,
          tabs: [
            { id: 't1', appId: ALPHA.id },
            { id: 't2', appId: BETA.id },
          ],
          activeTabId: 't1',
        },
      ],
      activeWorkspaceId: 'ws',
    },
    APPS,
    AREA,
  )
  const histories: Histories = new Map([['t1', createHistory('/stale')]])
  histories.clear()
  syncHistories(histories, state)
  for (const id of ['t1', 't2']) {
    assert.equal(histories.get(id)?.entries.length, 1)
    assert.equal(pathOf(histories, id), '/')
  }
})

test('navigateTab and goTab report whether anything changed', () => {
  const { state, histories } = fresh()
  const a = openAppAt(state, histories, ALPHA.id, null, APPS).tabId ?? ''
  assert.equal(goTab(histories, a, -1), false)
  assert.equal(navigateTab(histories, a, '/'), false)
  assert.equal(navigateTab(histories, a, '/x'), true)
  assert.equal(navigateTab(histories, a, '/x?f=1', { replace: true }), true)
  assert.equal(histories.get(a)?.entries.length, 2)
  assert.equal(navigateTab(histories, 'missing', '/x'), false)
})

test('back and forward without entries change nothing and close nothing (SC-005)', () => {
  const { state, histories } = fresh()
  const a = openAppAt(state, histories, ALPHA.id, null, APPS).tabId ?? ''
  const windowsBefore = structuredClone(state.windows)
  const historyBefore = histories.get(a)
  assert.equal(goTab(histories, a, -1), false)
  assert.equal(goTab(histories, a, 1), false)
  assert.equal(goTab(histories, a, -5), false)
  assert.deepEqual(state.windows, windowsBefore)
  assert.equal(histories.get(a), historyBefore)
})

test("switching the active tab exposes that tab's own history (US2 AS2)", () => {
  const { state, histories } = fresh()
  const a = openAppAt(state, histories, ALPHA.id, null, APPS).tabId ?? ''
  const windowId = state.windows[0]?.id ?? ''
  const b =
    addTabAt(state, histories, windowId, BETA.id, null, APPS).tabId ?? ''
  navigateTab(histories, a, '/deep')
  switchTab(state, windowId, a)
  assert.equal(state.windows[0]?.activeTabId, a)
  assert.equal(
    pathOf(histories, state.windows[0]?.activeTabId ?? null),
    '/deep',
  )
  switchTab(state, windowId, b)
  assert.equal(pathOf(histories, state.windows[0]?.activeTabId ?? null), '/')
})

test('skipTabEntry drops the current entry and moves on in travel direction (FR-027)', () => {
  const { state, histories } = fresh()
  const a = openAppAt(state, histories, ALPHA.id, null, APPS).tabId ?? ''
  navigateTab(histories, a, '/thread/1')
  navigateTab(histories, a, '/thread/2')
  navigateTab(histories, a, '/thread/3')
  goTab(histories, a, -1)
  assert.equal(skipTabEntry(histories, a, -1), true)
  assert.equal(pathOf(histories, a), '/thread/1')
  assert.equal(histories.get(a)?.entries.length, 3)
  goTab(histories, a, 1)
  assert.equal(skipTabEntry(histories, a, 1), true)
  assert.equal(pathOf(histories, a), '/thread/1')
})

test('skipTabEntry keeps a single-entry history and reports false', () => {
  const { state, histories } = fresh()
  const a = openAppAt(state, histories, ALPHA.id, '/thread/9', APPS).tabId ?? ''
  assert.equal(skipTabEntry(histories, a, -1), false)
  assert.equal(pathOf(histories, a), '/thread/9')
})

// ---------------------------------------------------------------------------
// System back (wm.system.back, FR-019)
// ---------------------------------------------------------------------------

test('system back closes an open window manager overlay first', () => {
  const { state, histories } = fresh()
  const a = openAppAt(state, histories, ALPHA.id, null, APPS).tabId ?? ''
  navigateTab(histories, a, '/x')
  assert.deepEqual(resolveSystemBack(state, histories, true), {
    kind: 'closeOverlay',
  })
})

test('system back goes back in the active tab of the top visible window', () => {
  const { state, histories } = fresh()
  const a = openAppAt(state, histories, ALPHA.id, null, APPS).tabId ?? ''
  const b = openAppAt(state, histories, BETA.id, null, APPS).tabId ?? ''
  navigateTab(histories, a, '/x')
  navigateTab(histories, b, '/y')
  assert.deepEqual(resolveSystemBack(state, histories, false), {
    kind: 'back',
    tabId: b,
  })
  minimizeWindow(state, state.windows[1]?.id ?? '')
  assert.deepEqual(resolveSystemBack(state, histories, false), {
    kind: 'back',
    tabId: a,
  })
})

test('system back without entries opens the window overview in compact mode only', () => {
  const { state, histories } = fresh()
  openAppAt(state, histories, ALPHA.id, null, APPS)
  assert.deepEqual(resolveSystemBack(state, histories, false), { kind: 'none' })
  state.compact = true
  assert.deepEqual(resolveSystemBack(state, histories, false), {
    kind: 'openWindowOverview',
  })
  const empty = fresh()
  empty.state.compact = true
  assert.deepEqual(resolveSystemBack(empty.state, empty.histories, false), {
    kind: 'openWindowOverview',
  })
})

// ---------------------------------------------------------------------------
// Handler registries (actions/handlers.ts)
// ---------------------------------------------------------------------------

const CONTEXT: ActionHandlerContext = {
  input: {},
  caller: { kind: 'user' },
  target: {},
}

test('global handlers register and resolve by action id', () => {
  const registry = createHandlerRegistry()
  registry.registerGlobal('a', () => 'ok')
  assert.equal(registry.globalHandler('a')?.(CONTEXT), 'ok')
  assert.equal(registry.globalHandler('b'), undefined)
})

test('a tab handler already registered resolves immediately', async () => {
  const registry = createHandlerRegistry()
  registry.registerTab('tab-1', 'chat.send', () => 'sent')
  const handler = await registry.awaitTab(['tab-1'], 'chat.send', 50)
  assert.equal(handler?.(CONTEXT), 'sent')
})

test('awaitTab resolves once the tab registers its handler', async () => {
  const registry = createHandlerRegistry()
  const pending = registry.awaitTab(['tab-1'], 'chat.send', 500)
  setTimeout(() => registry.registerTab('tab-1', 'chat.send', () => 'late'), 10)
  assert.equal((await pending)?.(CONTEXT), 'late')
})

test('awaitTab gives up after the timeout; unregister removes the handler', async () => {
  const registry = createHandlerRegistry()
  assert.equal(await registry.awaitTab(['tab-1'], 'chat.send', 20), null)
  const unregister = registry.registerTab('tab-1', 'chat.send', () => 'x')
  unregister()
  assert.equal(await registry.awaitTab(['tab-1'], 'chat.send', 20), null)
})

test('dropTab removes every handler of a closed tab', async () => {
  const registry = createHandlerRegistry()
  registry.registerTab('tab-1', 'chat.send', () => 'x')
  registry.registerTab('tab-1', 'chat.cancel', () => 'y')
  registry.dropTab('tab-1')
  assert.equal(await registry.awaitTab(['tab-1'], 'chat.cancel', 20), null)
})

// ---------------------------------------------------------------------------
// setTabHistory (spec 062: the file browser swaps the history of its active panel)
// ---------------------------------------------------------------------------

function twoEntries(): TabHistory {
  return push(createHistory('/device?p=%2Fa'), '/device?p=%2Fb', null)
}

test('setTabHistory replaces the history of a tab and nothing else', () => {
  const { state, histories } = fresh()
  const first = openAppAt(state, histories, ALPHA.id, null, APPS).tabId ?? ''
  const second = openAppAt(state, histories, BETA.id, null, APPS).tabId ?? ''
  const next = twoEntries()
  assert.equal(setTabHistory(histories, first, next), true)
  assert.equal(histories.get(first)?.entries.length, 2)
  assert.equal(histories.get(first)?.index, 1)
  assert.equal(pathOf(histories, first), '/device')
  assert.equal(pathOf(histories, second), '/')
})

test('setTabHistory stores a copy: changing the argument afterwards changes nothing', () => {
  const { state, histories } = fresh()
  const tabId = openAppAt(state, histories, ALPHA.id, null, APPS).tabId ?? ''
  const next = twoEntries()
  setTabHistory(histories, tabId, next)
  next.index = 0
  next.entries[1]!.location.query.p = '/changed'
  next.entries.pop()
  const stored = histories.get(tabId)
  assert.equal(stored?.index, 1)
  assert.equal(stored?.entries.length, 2)
  assert.equal(stored ? currentLocation(stored).query.p : null, '/b')
})

test('setTabHistory refuses a history that breaks the invariants and changes nothing', () => {
  const { state, histories } = fresh()
  const tabId =
    openAppAt(state, histories, ALPHA.id, '/models', APPS).tabId ?? ''
  const valid = twoEntries()
  const tooMany: TabHistory = {
    entries: Array.from({ length: MAX_HISTORY_ENTRIES + 1 }, () => ({
      location: { path: '/x', query: {} },
      title: null,
    })),
    index: 0,
  }
  const refused: TabHistory[] = [
    { entries: [], index: 0 },
    tooMany,
    { ...valid, index: -1 },
    { ...valid, index: valid.entries.length },
    { ...valid, index: 0.5 },
  ]
  for (const history of refused) {
    assert.equal(setTabHistory(histories, tabId, history), false)
    assert.equal(pathOf(histories, tabId), '/models')
    assert.equal(histories.get(tabId)?.entries.length, 1)
  }
  const full: TabHistory = { ...tooMany, entries: tooMany.entries.slice(1) }
  assert.equal(setTabHistory(histories, tabId, full), true)
  assert.equal(histories.get(tabId)?.entries.length, MAX_HISTORY_ENTRIES)
})

test('setTabHistory on an unknown tab returns false and adds no history', () => {
  const histories: Histories = new Map()
  assert.equal(setTabHistory(histories, 'gone', twoEntries()), false)
  assert.equal(histories.size, 0)
})
