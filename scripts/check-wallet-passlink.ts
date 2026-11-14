// Part of `pnpm check:wallet` (spec 092, FR-038): the link of a pinned shortcut names an opaque id
// and nothing else.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import { PASS_LINK_PREFIX, passIdFromLink } from '../src/lib/wallet/passLink.ts'
import { walletEntryPath } from '../src/lib/wallet/registry.ts'

const ID = '3f2b8c1e-9a44-4d6b-8f10-5c7e2d9a1b60'

test('a link to a pass names its id, in lower case', () => {
  assert.equal(passIdFromLink(`${PASS_LINK_PREFIX}${ID}`), ID)
  assert.equal(passIdFromLink(`${PASS_LINK_PREFIX}${ID.toUpperCase()}`), ID)
})

test('anything else is not a link to a pass', () => {
  for (const other of [
    undefined,
    null,
    42,
    '',
    PASS_LINK_PREFIX,
    `${PASS_LINK_PREFIX}pass-1`,
    `${PASS_LINK_PREFIX}${ID}/extra`,
    `${PASS_LINK_PREFIX}${ID}?code=1`,
    `${PASS_LINK_PREFIX}${ID}#x`,
    ` ${PASS_LINK_PREFIX}${ID}`,
    `haex-vault://contact/${ID}`,
    `haexvault://pass/${ID}`,
    `https://example.com/${PASS_LINK_PREFIX}${ID}`,
  ]) {
    assert.equal(passIdFromLink(other), null, String(other))
  }
})

test('the place a link opens is the pass in Wallet, built from the id alone', () => {
  const id = passIdFromLink(`${PASS_LINK_PREFIX}${ID}`)
  assert.equal(id === null ? null : walletEntryPath(id), `/entry/${ID}`)
})
