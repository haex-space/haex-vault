// Part of `pnpm check:extensions`: how the install dialog groups the declared permissions (spec 017
// FR-067, src/lib/extensions/permissionGroups.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  groupPermissions,
  hasWildcard,
} from '../src/lib/extensions/permissionGroups.ts'

const row = (kind: string, action: string, target: string) => ({
  kind,
  action,
  target,
})

test('permissions of one kind share a group, in order of first appearance', () => {
  const groups = groupPermissions([
    row('filesystem', 'read', '/a'),
    row('web', 'GET', 'https://example.org'),
    row('filesystem', 'write', '/b'),
  ])
  assert.deepEqual(
    groups.map((g) => [g.kind, g.rows.length]),
    [
      ['filesystem', 2],
      ['web', 1],
    ],
  )
  assert.deepEqual(
    groups[0]!.rows.map((r) => r.target),
    ['/a', '/b'],
  )
})

test('no permissions make no groups', () => {
  assert.deepEqual(groupPermissions([]), [])
})

test('a group with a target * or the web action * is wide', () => {
  assert.equal(hasWildcard(row('filesystem', 'read', '*')), true)
  assert.equal(hasWildcard(row('web', '*', 'example.org')), true)
  const [group] = groupPermissions([
    row('filesystem', 'read', '/a'),
    row('filesystem', 'read', '*'),
  ])
  assert.equal(group!.wide, true)
})

test('narrow targets and patterns ending in * are not wide', () => {
  const [group] = groupPermissions([
    row('filesystem', 'read', '/home/a/docs/*'),
    row('web', 'GET', '*.example.org'),
  ])
  assert.equal(group!.wide, false)
})

test('notifications and peers are never wide, whatever their target', () => {
  assert.equal(hasWildcard(row('notifications', 'show', '*')), false)
  assert.equal(hasWildcard(row('peers', 'connect', '*')), false)
})

test('only the group with the wildcard is marked', () => {
  const groups = groupPermissions([
    row('mail', 'send', '*'),
    row('passwords', 'read', 'haex-mail'),
  ])
  assert.deepEqual(
    groups.map((g) => [g.kind, g.wide]),
    [
      ['mail', true],
      ['passwords', false],
    ],
  )
})

test('agent tools are a group of their own and never wide', () => {
  const groups = groupPermissions([
    row('agentTool', 'read', 'list_accounts'),
    row('agentTool', 'destructive', 'send_mail'),
  ])
  assert.equal(groups.length, 1)
  assert.equal(groups[0]!.kind, 'agentTool')
  assert.equal(groups[0]!.wide, false)
})
