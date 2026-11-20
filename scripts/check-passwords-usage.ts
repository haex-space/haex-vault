// Part of `pnpm check:passwords` (spec 034-password-manager, FR-034): which entries about to be
// deleted a haex-vault function uses (src/lib/passwords/usage.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import { usageWarning } from '../src/lib/passwords/usage.ts'

test('with no function using an entry there is no warning', () => {
  assert.equal(usageWarning([]), null)
  assert.equal(
    usageWarning([
      { itemId: 'a', features: [] },
      { itemId: 'b', features: [] },
    ]),
    null,
  )
})

test('the warning names every function once, sorted, and the entries it uses', () => {
  const warning = usageWarning([
    { itemId: 'a', features: ['s3-storage'] },
    { itemId: 'b', features: [] },
    { itemId: 'c', features: ['backup', 's3-storage'] },
  ])
  assert.deepEqual(warning, {
    features: ['backup', 's3-storage'],
    itemIds: ['a', 'c'],
  })
})
