// Part of `pnpm check:settings` (spec 085, FR-009, FR-010): what a link text does to the link form.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  isLinkTextInput,
  serversTakenOver,
  withLinkText,
  type LinkFormFields,
} from '../src/lib/sync/linkForm.ts'

const empty: LinkFormFields = {
  code: '',
  vaultName: '',
  nostrRelays: [],
  irohRelays: [],
  disabledRelays: [],
  serversFromLink: false,
}

const withServers = {
  code: 'ABCD-EFGH-IJKL-MNOP-QRST-UV',
  vaultName: 'Privat',
  servers: {
    nostrRelays: ['wss://relay.example.org'],
    irohRelays: ['https://iroh.example.org'],
    disabled: ['wss://nos.lol'],
  },
  droppedServers: 0,
}

test('a link text fills code, an empty vault name and the servers', () => {
  const form = withLinkText(empty, withServers)

  assert.equal(form.code, 'ABCD-EFGH-IJKL-MNOP-QRST-UV')
  assert.equal(form.vaultName, 'Privat')
  assert.deepEqual(form.nostrRelays, ['wss://relay.example.org'])
  assert.deepEqual(form.irohRelays, ['https://iroh.example.org'])
  assert.deepEqual(form.disabledRelays, ['wss://nos.lol'])
  assert.equal(form.serversFromLink, true)
  assert.equal(serversTakenOver(form), 3)
})

test('a vault name the user typed is kept', () => {
  const form = withLinkText({ ...empty, vaultName: 'Mein Tresor' }, withServers)

  assert.equal(form.vaultName, 'Mein Tresor')
})

test('a second text replaces code and servers', () => {
  const first = withLinkText(empty, withServers)
  const second = withLinkText(first, {
    ...withServers,
    code: 'ZZZZ-ZZZZ-ZZZZ-ZZZZ-ZZZZ-ZA',
    servers: { nostrRelays: [], irohRelays: [], disabled: ['wss://nos.lol'] },
  })

  assert.equal(second.code, 'ZZZZ-ZZZZ-ZZZZ-ZZZZ-ZZZZ-ZA')
  assert.deepEqual(second.nostrRelays, [])
  assert.deepEqual(second.disabledRelays, ['wss://nos.lol'])
  // The name of the first text was filled in, so it counts as the user's now.
  assert.equal(second.vaultName, 'Privat')
})

test('a plain code leaves the servers the user entered alone', () => {
  const typed = { ...empty, nostrRelays: ['wss://mine.example.org'] }

  const form = withLinkText(typed, {
    code: 'ABCD-EFGH-IJKL-MNOP-QRST-UV',
    droppedServers: 0,
  })

  assert.deepEqual(form.nostrRelays, ['wss://mine.example.org'])
  assert.equal(form.serversFromLink, false)
  assert.equal(form.code, 'ABCD-EFGH-IJKL-MNOP-QRST-UV')
})

test('a plain code drops the servers an earlier link text brought', () => {
  const first = withLinkText(empty, withServers)

  const form = withLinkText(first, {
    code: 'ZZZZ-ZZZZ-ZZZZ-ZZZZ-ZZZZ-ZA',
    droppedServers: 0,
  })

  assert.equal(form.serversFromLink, false)
  assert.equal(serversTakenOver(form), 0)
})

test('only a link or something longer than a code goes to the backend', () => {
  assert.equal(isLinkTextInput('haex-vault://link?v=1&c=ABC'), true)
  assert.equal(isLinkTextInput('  HAEX-VAULT://link?v=1'), true)
  assert.equal(isLinkTextInput('ABCD-EFGH-IJKL-MNOP-QRST-UV'), false)
  assert.equal(isLinkTextInput('abcd efgh'), false)
  assert.equal(isLinkTextInput('ABCD-EFGH-IJKL-MNOP-QRST-UVWX-YZ23'), true)
  assert.equal(isLinkTextInput(''), false)
})
