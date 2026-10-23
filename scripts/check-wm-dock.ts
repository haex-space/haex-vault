// Part of `pnpm check:wm-state` (spec 045-dock): the dock's pure logic (src/lib/wm/dock.ts) —
// reading its preferences and normalizing the stored entries (data-model.md).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  DEFAULT_DOCK_ITEMS,
  DEFAULT_DOCK_PLACEMENT,
  dockActivation,
  effectivePlacement,
  normalizeDockItems,
  parseDockItems,
  parseDockPlacement,
  resolveDockEntries,
  serializeDockItems,
  wheelLayout,
  WHEEL_ITEM_SIZE,
  type DockAlign,
  type DockEdge,
  type DockItem,
  type DockPlacement,
} from '../src/lib/wm/dock.ts'
import { WM_APPS } from '../src/lib/wm/apps.ts'
import type { WmWindow } from '../src/lib/wm/types.ts'
import { ALPHA, APPS, BETA } from './lib/wm-fixtures.ts'

const launcher: DockItem = { kind: 'control', id: 'launcher' }
const windows: DockItem = { kind: 'control', id: 'windows' }
const alpha: DockItem = { kind: 'app', appId: ALPHA.id }
const beta: DockItem = { kind: 'app', appId: BETA.id }

// ---------------------------------------------------------------------------
// The defaults (FR-003)
// ---------------------------------------------------------------------------

test('the dock defaults to the launcher and the system apps, all available', () => {
  assert.deepEqual(normalizeDockItems(DEFAULT_DOCK_ITEMS, WM_APPS), [
    { kind: 'control', id: 'launcher', available: true },
    { kind: 'app', appId: 'system.files', available: true },
    { kind: 'app', appId: 'system.chat', available: true },
    { kind: 'app', appId: 'system.passwords', available: true },
    { kind: 'app', appId: 'system.settings', available: true },
  ])
})

// ---------------------------------------------------------------------------
// Reading the preferences
// ---------------------------------------------------------------------------

test('parseDockItems reports a missing or non-list value as unreadable', () => {
  assert.equal(parseDockItems(null), null)
  assert.equal(parseDockItems('x'), null)
  assert.equal(parseDockItems('{}'), null)
})

test('parseDockItems keeps valid elements and drops malformed ones', () => {
  const raw = JSON.stringify([
    launcher,
    { kind: 'control', id: 'taskbar' },
    { kind: 'app' },
    { kind: 'app', appId: '' },
    'alpha',
    alpha,
  ])
  assert.deepEqual(parseDockItems(raw), [launcher, alpha])
})

test('parseDockPlacement falls back field by field', () => {
  assert.deepEqual(parseDockPlacement(null), DEFAULT_DOCK_PLACEMENT)
  assert.deepEqual(parseDockPlacement('nope'), DEFAULT_DOCK_PLACEMENT)
  assert.deepEqual(
    parseDockPlacement(
      JSON.stringify({
        style: 'wheel',
        edge: 'diagonal',
        align: 'end',
        mode: 7,
      }),
    ),
    { style: 'wheel', edge: 'bottom', align: 'end', mode: 'reserved' },
  )
})

// ---------------------------------------------------------------------------
// Normalizing (data-model.md, "Regeln beim Lesen")
// ---------------------------------------------------------------------------

test('normalizeDockItems keeps the stored order and marks known items available', () => {
  assert.deepEqual(normalizeDockItems([launcher, beta, windows, alpha], APPS), [
    { ...launcher, available: true },
    { ...beta, available: true },
    { ...windows, available: true },
    { ...alpha, available: true },
  ])
})

test('normalizeDockItems keeps the first of duplicate controls and apps', () => {
  assert.deepEqual(
    normalizeDockItems([launcher, alpha, windows, alpha, windows], APPS),
    [
      { ...launcher, available: true },
      { ...alpha, available: true },
      { ...windows, available: true },
    ],
  )
})

test('normalizeDockItems puts a missing launcher first', () => {
  assert.deepEqual(normalizeDockItems([alpha], APPS), [
    { ...launcher, available: true },
    { ...alpha, available: true },
  ])
})

test('normalizeDockItems keeps an app unknown on this device, marked unavailable', () => {
  const gone: DockItem = { kind: 'app', appId: 'extension.gone' }
  assert.deepEqual(normalizeDockItems([launcher, gone], APPS), [
    { ...launcher, available: true },
    { ...gone, available: false },
  ])
})

test('normalizeDockItems maps a legacy app id to its app and dedupes against it', () => {
  const settings: DockItem = { kind: 'app', appId: 'system.settings' }
  const federation: DockItem = { kind: 'app', appId: 'system.federation' }
  const apps = [
    ...APPS,
    { ...ALPHA, id: 'system.settings', titleKey: 'settings' },
  ]
  assert.deepEqual(normalizeDockItems([launcher, federation, settings], apps), [
    { ...launcher, available: true },
    { ...settings, available: true },
  ])
})

test('serializeDockItems writes unavailable items back, without the device-local flag', () => {
  const gone: DockItem = { kind: 'app', appId: 'extension.gone' }
  const written = serializeDockItems(normalizeDockItems([gone, alpha], APPS))
  assert.deepEqual(JSON.parse(written), [launcher, gone, alpha])
})

// ---------------------------------------------------------------------------
// Entries to show (resolveDockEntries)
// ---------------------------------------------------------------------------

function windowWith(
  id: string,
  workspaceId: string,
  tabs: { id: string; appId: string }[],
): WmWindow {
  return {
    id,
    workspaceId,
    x: 0,
    y: 0,
    width: 400,
    height: 300,
    minimized: false,
    maximized: false,
    stack: 0,
    tabs,
    activeTabId: tabs[0]?.id ?? '',
  }
}

test('resolveDockEntries shows available items in order and hides unavailable apps', () => {
  const gone: DockItem = { kind: 'app', appId: 'extension.gone' }
  const items = normalizeDockItems([launcher, gone, beta, windows], APPS)
  assert.deepEqual(resolveDockEntries(items, []), [
    { kind: 'control', id: 'launcher' },
    { kind: 'app', appId: BETA.id, pinned: true, instances: [] },
    { kind: 'control', id: 'windows' },
  ])
})

test("resolveDockEntries collects an app's tabs across windows and workspaces", () => {
  const items = normalizeDockItems([launcher, alpha], APPS)
  const open = [
    windowWith('w1', 'ws-1', [
      { id: 't1', appId: ALPHA.id },
      { id: 't2', appId: BETA.id },
    ]),
    windowWith('w2', 'ws-2', [{ id: 't3', appId: ALPHA.id }]),
  ]
  const [, entry] = resolveDockEntries(items, open)
  assert.deepEqual(entry, {
    kind: 'app',
    appId: ALPHA.id,
    pinned: true,
    instances: [
      { tabId: 't1', windowId: 'w1', workspaceId: 'ws-1' },
      { tabId: 't3', windowId: 'w2', workspaceId: 'ws-2' },
    ],
  })
})

test('resolveDockEntries appends running apps that are not pinned, in tab order', () => {
  const gamma = 'test.gamma'
  const items = normalizeDockItems([launcher, alpha], APPS)
  const open = [
    windowWith('w1', 'ws-2', [
      { id: 't1', appId: gamma },
      { id: 't2', appId: ALPHA.id },
    ]),
    windowWith('w2', 'ws-1', [
      { id: 't3', appId: BETA.id },
      { id: 't4', appId: gamma },
    ]),
  ]
  assert.deepEqual(
    resolveDockEntries(items, open).map((entry) =>
      entry.kind === 'app'
        ? [entry.appId, entry.pinned, entry.instances.length]
        : [entry.id],
    ),
    [['launcher'], [ALPHA.id, true, 1], [gamma, false, 2], [BETA.id, false, 1]],
  )
})

test('resolveDockEntries shows a pinned running app once, at its pinned place', () => {
  const items = normalizeDockItems([alpha, launcher], APPS)
  const open = [windowWith('w1', 'ws-1', [{ id: 't1', appId: ALPHA.id }])]
  const apps = resolveDockEntries(items, open).filter(
    (entry) => entry.kind === 'app',
  )
  assert.equal(apps.length, 1)
  assert.equal(resolveDockEntries(items, open)[0]?.kind, 'app')
})

test('dockActivation opens, focuses the one instance, or lets the user choose', () => {
  const one = { tabId: 't1', windowId: 'w1', workspaceId: 'ws-1' }
  const two = { tabId: 't2', windowId: 'w2', workspaceId: 'ws-2' }
  assert.deepEqual(dockActivation([]), { kind: 'open' })
  assert.deepEqual(dockActivation([one]), { kind: 'focus', tabId: 't1' })
  assert.deepEqual(dockActivation([one, two]), { kind: 'choose' })
})

// ---------------------------------------------------------------------------
// Effective placement (data-model.md)
// ---------------------------------------------------------------------------

test('effectivePlacement keeps the choice outside compact mode', () => {
  const placement: DockPlacement = {
    style: 'bar',
    edge: 'left',
    align: 'end',
    mode: 'autohide',
  }
  assert.deepEqual(effectivePlacement(placement, false), placement)
})

test('effectivePlacement puts the bar at the bottom, reserving space, in compact mode', () => {
  for (const edge of ['top', 'bottom', 'left', 'right'] as const) {
    for (const mode of ['reserved', 'floating', 'autohide'] as const) {
      assert.deepEqual(
        effectivePlacement({ style: 'bar', edge, align: 'start', mode }, true),
        { style: 'bar', edge: 'bottom', align: 'center', mode: 'reserved' },
      )
    }
  }
})

test('effectivePlacement puts the wheel into a bottom corner in compact mode', () => {
  const wheel = (align: 'start' | 'center' | 'end'): DockPlacement => ({
    style: 'wheel',
    edge: 'top',
    align,
    mode: 'floating',
  })
  assert.equal(effectivePlacement(wheel('start'), true).align, 'start')
  assert.equal(effectivePlacement(wheel('center'), true).align, 'end')
  assert.equal(effectivePlacement(wheel('end'), true).align, 'end')
  assert.equal(effectivePlacement(wheel('start'), true).edge, 'bottom')
})

// ---------------------------------------------------------------------------
// Wheel geometry (research R6)
// ---------------------------------------------------------------------------

const EDGES: readonly DockEdge[] = ['top', 'bottom', 'left', 'right']
const ALIGNS: readonly DockAlign[] = ['start', 'center', 'end']
const EPSILON = 1e-6

test('wheelLayout never lets two entries overlap, at any position and count', () => {
  for (const edge of EDGES) {
    for (const align of ALIGNS) {
      for (let count = 1; count <= 20; count += 1) {
        const offsets = wheelLayout(count, edge, align)
        assert.equal(offsets.length, count)
        for (let i = 0; i < offsets.length; i += 1) {
          for (let j = i + 1; j < offsets.length; j += 1) {
            const a = offsets[i]!
            const b = offsets[j]!
            const distance = Math.hypot(a.x - b.x, a.y - b.y)
            assert.ok(
              distance >= WHEEL_ITEM_SIZE - EPSILON,
              `${edge}/${align} with ${count}: entries ${i} and ${j} are ${distance} apart`,
            )
          }
        }
      }
    }
  }
})

test('wheelLayout fans out into the screen, away from the edge and the corner', () => {
  for (const align of ALIGNS) {
    for (const { y } of wheelLayout(20, 'bottom', align))
      assert.ok(y <= EPSILON, `bottom/${align} goes below the edge`)
    for (const { y } of wheelLayout(20, 'top', align))
      assert.ok(y >= -EPSILON, `top/${align} goes above the edge`)
    for (const { x } of wheelLayout(20, 'left', align))
      assert.ok(x >= -EPSILON, `left/${align} goes past the edge`)
    for (const { x } of wheelLayout(20, 'right', align))
      assert.ok(x <= EPSILON, `right/${align} goes past the edge`)
  }
  // Corners: both coordinates point inward.
  for (const { x, y } of wheelLayout(20, 'bottom', 'end'))
    assert.ok(x <= EPSILON && y <= EPSILON)
  for (const { x, y } of wheelLayout(20, 'top', 'start'))
    assert.ok(x >= -EPSILON && y >= -EPSILON)
  for (const { x, y } of wheelLayout(20, 'left', 'end'))
    assert.ok(x >= -EPSILON && y <= EPSILON)
  for (const { x, y } of wheelLayout(20, 'right', 'start'))
    assert.ok(x <= EPSILON && y >= -EPSILON)
})

test('wheelLayout adds outer rings once the inner one is full', () => {
  const radii = (count: number) =>
    new Set(
      wheelLayout(count, 'bottom', 'end').map(({ x, y }) =>
        Math.round(Math.hypot(x, y)),
      ),
    ).size
  assert.equal(radii(1), 1)
  assert.ok(radii(12) >= 2)
  assert.ok(radii(20) > radii(6))
})
