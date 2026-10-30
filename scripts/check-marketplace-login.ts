// Part of `pnpm check:marketplace` (spec 069 US5): what the form sends for a login, when it asks
// for one, and what it refuses before anything is sent.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  emptyLogin,
  isPlainHttp,
  loginFields,
  loginProblem,
  needsLoginInput,
  type KeptLogin,
  type LoginDraft,
} from '../src/lib/marketplace/loginForm.ts'

const draft = (over: Partial<LoginDraft> = {}): LoginDraft => ({
  ...emptyLogin(),
  ...over,
})
const kept = (over: Partial<KeptLogin> = {}): KeptLogin => ({
  authType: 'bearer',
  credentialsItemId: 'item-1',
  ...over,
})
const https = 'https://market.example'

test('no login sends no login fields', () => {
  assert.deepEqual(loginFields(draft(), null), { authType: 'none' })
  assert.equal(needsLoginInput(draft(), kept()), false)
  assert.equal(loginProblem(draft(), 'http://plain.example', null), null)
})

test('a saved login is kept until the user asks for another', () => {
  const same = draft({ authType: 'bearer' })
  assert.equal(needsLoginInput(same, kept()), false)
  assert.deepEqual(loginFields(same, kept()), {
    authType: 'bearer',
    credentialsItemId: 'item-1',
  })
  assert.equal(loginProblem(same, https, kept()), null)

  const replace = draft({ authType: 'bearer', replace: true })
  assert.equal(needsLoginInput(replace, kept()), true)
  assert.equal(loginProblem(replace, https, kept()), 'secret')
})

test('a changed auth type or a row without an entry asks for a login', () => {
  assert.equal(needsLoginInput(draft({ authType: 'basic' }), kept()), true)
  assert.equal(needsLoginInput(draft({ authType: 'bearer' }), null), true)
  assert.equal(
    needsLoginInput(
      draft({ authType: 'bearer' }),
      kept({ credentialsItemId: undefined }),
    ),
    true,
  )
})

test('a typed bearer token travels as a secret without a user name', () => {
  const typed = draft({
    authType: 'bearer',
    username: 'ignored',
    secret: 'tok',
  })
  assert.equal(loginProblem(typed, https, null), null)
  assert.deepEqual(loginFields(typed, null), {
    authType: 'bearer',
    newCredentials: { username: undefined, secret: 'tok' },
  })
})

test('a basic login needs both a user name and a password', () => {
  const nameless = draft({ authType: 'basic', secret: 'pw' })
  assert.equal(loginProblem(nameless, https, null), 'username')
  const full = draft({ authType: 'basic', username: ' ada ', secret: 'pw' })
  assert.equal(loginProblem(full, https, null), null)
  assert.deepEqual(loginFields(full, null), {
    authType: 'basic',
    newCredentials: { username: 'ada', secret: 'pw' },
  })
})

test('a chosen entry sends only its id', () => {
  const none = draft({ authType: 'basic', source: 'entry' })
  assert.equal(loginProblem(none, https, null), 'entry')
  const chosen = draft({ authType: 'basic', source: 'entry', entryId: 'e1' })
  assert.equal(loginProblem(chosen, https, null), null)
  assert.deepEqual(loginFields(chosen, null), {
    authType: 'basic',
    credentialsItemId: 'e1',
  })
})

test('a login with http is refused whatever else is filled in (US5-6)', () => {
  const typed = draft({ authType: 'bearer', secret: 'tok' })
  assert.equal(loginProblem(typed, 'http://plain.example', null), 'insecure')
  assert.equal(loginProblem(typed, ' HTTP://plain.example', null), 'insecure')
  assert.equal(
    loginProblem(draft({ authType: 'bearer' }), 'http://x.example', kept()),
    'insecure',
  )
})

test('a loopback http address is left to the host (debug builds take it)', () => {
  assert.equal(isPlainHttp('http://127.0.0.1:8080/team'), false)
  assert.equal(isPlainHttp('http://localhost:3000'), false)
  assert.equal(isPlainHttp('http://[::1]:3000'), false)
  assert.equal(isPlainHttp('http://192.168.1.5:3000'), true)
  assert.equal(isPlainHttp('https://market.example'), false)
  assert.equal(
    loginProblem(
      draft({ authType: 'bearer', secret: 'tok' }),
      'http://127.0.0.1:9',
      null,
    ),
    null,
  )
})
