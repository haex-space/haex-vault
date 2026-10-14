// Part of `pnpm check:storage` (spec 038, spec 050, spec 052 US1): the credentials of the storage
// forms (src/components/settings/storage/credentials.ts). Each protocol and way of signing in gives
// its kind, a chosen entry sends only its id, and an incomplete draft is not complete.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  clearSecrets,
  credentialsComplete,
  credentialsInput,
  emptyCredentials,
} from '../src/components/settings/storage/credentials.ts'

test('S3 takes the access key with an optional token', () => {
  const draft = {
    ...emptyCredentials(),
    accessKeyId: ' AKID ',
    secretAccessKey: 'secret',
  }
  assert.deepEqual(credentialsInput('s3', draft), {
    kind: 's3',
    accessKeyId: 'AKID',
    secretAccessKey: 'secret',
    sessionToken: undefined,
  })
  assert.equal(credentialsComplete('s3', draft), true)
  assert.equal(
    credentialsComplete('s3', { ...draft, secretAccessKey: '' }),
    false,
  )
})

test('WebDAV and SFTP take a password, SFTP also a key', () => {
  const draft = { ...emptyCredentials(), username: ' anna ', password: 'pw' }
  for (const protocol of ['webdav', 'sftp'] as const) {
    assert.deepEqual(credentialsInput(protocol, draft), {
      kind: 'password',
      username: 'anna',
      password: 'pw',
    })
    assert.equal(credentialsComplete(protocol, draft), true)
    assert.equal(
      credentialsComplete(protocol, { ...draft, username: ' ' }),
      false,
    )
  }
  const key = { ...draft, authMethod: 'key' as const, privateKey: 'KEY' }
  assert.deepEqual(credentialsInput('sftp', key), {
    kind: 'sshKey',
    username: 'anna',
    privateKey: 'KEY',
    passphrase: undefined,
  })
  assert.deepEqual(credentialsInput('sftp', { ...key, keyId: 'k1' }), {
    kind: 'generatedKey',
    username: 'anna',
    keyId: 'k1',
  })
  assert.equal(
    credentialsComplete('sftp', { ...key, privateKey: '', keyId: '' }),
    false,
  )
})

test('a chosen entry sends only its id, for every protocol', () => {
  const draft = {
    ...emptyCredentials(),
    source: 'entry' as const,
    entryId: 'e1',
    username: 'typed before',
    password: 'typed before',
  }
  for (const protocol of ['s3', 'webdav', 'sftp'] as const) {
    assert.deepEqual(credentialsInput(protocol, draft), {
      kind: 'entry',
      itemId: 'e1',
    })
    assert.equal(credentialsComplete(protocol, draft), true)
    assert.equal(
      credentialsComplete(protocol, { ...draft, entryId: '' }),
      false,
    )
  }
})

test('clearing forgets every secret and keeps the choice', () => {
  const draft = {
    ...emptyCredentials(),
    source: 'entry' as const,
    entryId: 'e1',
    username: 'anna',
    secretAccessKey: 's',
    sessionToken: 't',
    password: 'p',
    privateKey: 'k',
    passphrase: 'pp',
    keyId: 'id',
    publicKey: 'pub',
  }
  clearSecrets(draft)
  assert.deepEqual(draft, {
    ...emptyCredentials(),
    source: 'entry',
    entryId: 'e1',
    username: 'anna',
  })
})
