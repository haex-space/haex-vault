// Part of `pnpm check:wm-state`: opening an app in the background for a call of the agent (spec 018,
// research R9, `openAppInBackground` in src/lib/wm/layoutState.ts). The app gets a minimized window
// and the user's window, workspace and focus stay as they are; an instance already open wins.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import type { AppDefinition } from '../src/lib/wm/apps.ts'
import {
  createWorkspace,
  openApp,
  openAppInBackground,
  switchWorkspace,
} from '../src/lib/wm/layoutState.ts'
import { ALPHA, APPS, BETA, emptyState } from './lib/wm-fixtures.ts'

const MULTI: AppDefinition = {
  ...BETA,
  id: 'test.multi',
  titleKey: 'test.multi',
  multiInstance: true,
}
const WITH_MULTI = [...APPS, MULTI]

test('an app not open yet gets a new minimized window; the active window stays active', () => {
  const state = emptyState()
  openApp(state, ALPHA.id, APPS)
  const active = state.activeWindowId

  const tabId = openAppInBackground(state, BETA.id, APPS)

  assert.equal(state.windows.length, 2)
  const opened = state.windows.find((w) => w.tabs[0]?.appId === BETA.id)
  assert.ok(opened)
  assert.equal(opened.minimized, true)
  assert.equal(opened.activeTabId, tabId)
  assert.equal(opened.workspaceId, state.activeWorkspaceId)
  assert.equal(state.activeWindowId, active)
})

test('with no window open the background window leaves no window active', () => {
  const state = emptyState()
  openAppInBackground(state, ALPHA.id, APPS)
  assert.equal(state.windows[0]?.minimized, true)
  assert.equal(state.activeWindowId, null)
})

test('an open instance wins and nothing changes, whether visible or minimized', () => {
  const state = emptyState()
  openApp(state, ALPHA.id, APPS)
  openApp(state, BETA.id, APPS)
  const before = structuredClone(state)

  const tabId = openAppInBackground(state, ALPHA.id, APPS)

  assert.equal(tabId, state.windows[0]?.tabs[0]?.id)
  assert.deepEqual(state, before)
})

test('a multi-instance app also reuses its open tab in the active workspace', () => {
  const state = emptyState()
  openApp(state, MULTI.id, WITH_MULTI)
  const tabId = openAppInBackground(state, MULTI.id, WITH_MULTI)
  assert.equal(state.windows.length, 1)
  assert.equal(tabId, state.windows[0]?.tabs[0]?.id)
})

test('a singleton open in another workspace cannot serve and is not opened twice', () => {
  const state = emptyState()
  openApp(state, ALPHA.id, APPS)
  const other = createWorkspace(state, 'ws-2')
  switchWorkspace(state, other.id)

  assert.equal(openAppInBackground(state, ALPHA.id, APPS), null)
  assert.equal(state.windows.length, 1)
})

test('a multi-instance app open only in another workspace gets a background window here', () => {
  const state = emptyState()
  openApp(state, MULTI.id, WITH_MULTI)
  const other = createWorkspace(state, 'ws-2')
  switchWorkspace(state, other.id)

  const tabId = openAppInBackground(state, MULTI.id, WITH_MULTI)

  const opened = state.windows.find((w) => w.activeTabId === tabId)
  assert.equal(opened?.workspaceId, other.id)
  assert.equal(opened?.minimized, true)
})

test('an unknown app opens nothing', () => {
  const state = emptyState()
  assert.equal(openAppInBackground(state, 'nonexistent.app', APPS), null)
  assert.equal(state.windows.length, 0)
})

test('the compact layout gets the same minimized window, without switching to it', () => {
  const state = emptyState()
  state.compact = true
  openApp(state, ALPHA.id, APPS)
  const active = state.activeWindowId
  openAppInBackground(state, BETA.id, APPS)
  assert.equal(state.activeWindowId, active)
  assert.equal(state.windows.at(-1)?.minimized, true)
})
