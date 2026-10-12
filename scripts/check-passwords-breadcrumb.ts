// Part of `pnpm check:passwords` (spec 036, FR-010): the breadcrumbs above the list
// (src/lib/passwords/breadcrumb.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  breadcrumb,
  collapseCrumbs,
  type Crumb,
} from '../src/lib/passwords/breadcrumb.ts'

const GROUPS = [
  { id: 'work', name: 'Arbeit', parentId: null, sortOrder: 0 },
  { id: 'server', name: 'Server', parentId: 'work', sortOrder: 0 },
  { id: 'db', name: 'DB', parentId: 'server', sortOrder: 0 },
  { id: 'trash', name: null, parentId: null, sortOrder: 0 },
  { id: 'loop-a', name: 'A', parentId: 'loop-b', sortOrder: 0 },
  { id: 'loop-b', name: 'B', parentId: 'loop-a', sortOrder: 0 },
]

const labels = (crumbs: readonly Crumb[]) =>
  crumbs.map((crumb) =>
    crumb.kind === 'root'
      ? 'root'
      : crumb.kind === 'folder'
        ? (crumb.name ?? '?')
        : `view:${crumb.view}${crumb.name ? `:${crumb.name}` : ''}`,
  )

test('the top level is the root alone, and it is not a target', () => {
  const crumbs = breadcrumb(GROUPS, { kind: 'folder', id: null })
  assert.deepEqual(labels(crumbs), ['root'])
  assert.equal(crumbs[0]!.target, false)
})

test('a nested folder lists root, ancestors and itself; only the last is no target', () => {
  const crumbs = breadcrumb(GROUPS, { kind: 'folder', id: 'db' })
  assert.deepEqual(labels(crumbs), ['root', 'Arbeit', 'Server', 'DB'])
  assert.deepEqual(
    crumbs.map((crumb) => crumb.target),
    [true, true, true, false],
  )
  assert.deepEqual(
    crumbs.map((crumb) => (crumb.kind === 'folder' ? crumb.id : null)),
    [null, 'work', 'server', 'db'],
  )
})

test('an unknown folder still shows a part, and a loop in the data ends', () => {
  assert.deepEqual(labels(breadcrumb(GROUPS, { kind: 'folder', id: 'gone' })), [
    'root',
    '?',
  ])
  const looped = breadcrumb(GROUPS, { kind: 'folder', id: 'loop-a' })
  assert.deepEqual(labels(looped), ['root', 'B', 'A'])
})

test('trash, tag and search show the name of the view instead', () => {
  assert.deepEqual(labels(breadcrumb(GROUPS, { kind: 'trash' })), [
    'view:trash',
  ])
  assert.deepEqual(
    labels(breadcrumb(GROUPS, { kind: 'tag', name: 'privat' })),
    ['view:tag:privat'],
  )
  const search = breadcrumb(GROUPS, { kind: 'search' })
  assert.deepEqual(labels(search), ['view:search'])
  assert.equal(search[0]!.target, false)
})

test('an open entry shows the path to its folder, every part a target', () => {
  const crumbs = breadcrumb(GROUPS, { kind: 'entry', groupId: 'server' })
  assert.deepEqual(labels(crumbs), ['root', 'Arbeit', 'Server'])
  assert.ok(crumbs.every((crumb) => crumb.target))
  const top = breadcrumb(GROUPS, { kind: 'entry', groupId: null })
  assert.deepEqual(labels(top), ['root'])
  assert.equal(top[0]!.target, true)
})

test('an entry in the trash or in a folder below it shows the trash', () => {
  const nested = [
    ...GROUPS,
    { id: 'old', name: 'Alt', parentId: 'trash', sortOrder: 0 },
  ]
  for (const groupId of ['trash', 'old']) {
    const crumbs = breadcrumb(nested, { kind: 'entry', groupId })
    assert.deepEqual(labels(crumbs), ['view:trash'])
    assert.equal(crumbs[0]!.target, false)
  }
})

test('a long path keeps the first and the last parts and puts the middle into the overflow', () => {
  const crumbs = breadcrumb(GROUPS, { kind: 'folder', id: 'db' })
  const short = collapseCrumbs(crumbs, 3)
  assert.deepEqual(labels(short.visible), ['root', 'DB'])
  assert.deepEqual(labels(short.hidden), ['Arbeit', 'Server'])
  // With the overflow the row shows at most three parts: root, "…", the last one.
  assert.ok(short.visible.length + 1 <= 3)
  const roomy = collapseCrumbs(crumbs, 4)
  assert.deepEqual(labels(roomy.visible), labels(crumbs))
  assert.deepEqual(roomy.hidden, [])
})

test('a path that fits is not collapsed', () => {
  const crumbs = breadcrumb(GROUPS, { kind: 'folder', id: 'work' })
  const result = collapseCrumbs(crumbs, 3)
  assert.deepEqual(labels(result.visible), ['root', 'Arbeit'])
  assert.deepEqual(result.hidden, [])
})

test('with room for four, two parts stay at the end', () => {
  const deep = [...GROUPS, { id: 'x', name: 'X', parentId: 'db', sortOrder: 0 }]
  const crumbs = breadcrumb(deep, { kind: 'folder', id: 'x' })
  const result = collapseCrumbs(crumbs, 4)
  assert.deepEqual(labels(result.visible), ['root', 'DB', 'X'])
  assert.deepEqual(labels(result.hidden), ['Arbeit', 'Server'])
})
