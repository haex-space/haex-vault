// Part of `pnpm check:extensions`: which permission wildcards allow everything of their kind
// (src/lib/extensions/permissionScope.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  everythingKey,
  hasTarget,
  isEveryAction,
} from '../src/lib/extensions/permissionScope.ts'

test('a target * allows everything of its kind', () => {
  assert.equal(everythingKey('filesystem', 'readWrite', '*'), 'filesystem')
  assert.equal(everythingKey('web', 'GET', '*'), 'web')
  assert.equal(everythingKey('mail', 'send', '*'), 'mail')
  assert.equal(everythingKey('passwords', 'read', '*'), 'passwords')
  assert.equal(everythingKey('shell', 'execute', '*'), 'shell')
  assert.equal(everythingKey('remoteStorage', 'read', '*'), 'remoteStorage')
  assert.equal(everythingKey('contacts', 'read', '*'), 'contacts')
})

test('proposing storages on any server reads differently from using every storage', () => {
  assert.equal(everythingKey('remoteStorage', 'add', '*'), 'remoteStorageAdd')
})

test('a narrow target or a pattern ending in * is not everything', () => {
  assert.equal(everythingKey('filesystem', 'read', '/home/a/docs/*'), null)
  assert.equal(everythingKey('web', '*', '*.example.org'), null)
  assert.equal(everythingKey('web', 'GET', 'https://example.org/*'), null)
  assert.equal(everythingKey('passwords', 'readWrite', 'haex-mail'), null)
  assert.equal(everythingKey('contacts', 'read', 'Arbeit'), null)
})

test('notifications, peers and database never count as everything', () => {
  assert.equal(everythingKey('notifications', 'show', '*'), null)
  assert.equal(everythingKey('peers', 'connect', '*'), null)
  assert.equal(hasTarget('peers'), false)
  assert.equal(everythingKey('database', 'read', '*'), null)
  assert.equal(hasTarget('notifications'), false)
  assert.equal(hasTarget('filesystem'), true)
})

test('only the web action * means every action', () => {
  assert.equal(isEveryAction('web', '*'), true)
  assert.equal(isEveryAction('web', 'GET'), false)
  assert.equal(isEveryAction('mail', '*'), false)
})
