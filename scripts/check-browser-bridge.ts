// `pnpm check:browser-bridge` (spec 049, research R18): the WebCrypto side of the envelope fixture.
// The Rust test in `envelope_tests.rs` opens the fixture's request and seals the same response byte
// for byte; here WebCrypto, as haex-pass-browser calls it, opens that response with the client's key.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { build, FIXTURE, open } from './fixtures/browser-bridge-envelope.ts'
import type { Fixture } from './fixtures/browser-bridge-envelope.ts'

const fixture = JSON.parse(readFileSync(FIXTURE, 'utf8')) as Fixture

test('the fixture is what the generator writes', async () => {
  assert.deepEqual(fixture, await build())
})

test('WebCrypto opens the response holzi seals with the client key', async () => {
  assert.equal(
    await open(fixture.client.secret, fixture.response),
    fixture.response.plaintext,
  )
})

test('a response to another client does not open', async () => {
  await assert.rejects(open(fixture.server.secret, fixture.response))
})
