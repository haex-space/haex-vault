// Part of `pnpm check:passwords`: the stash that keeps the unsaved draft of the entry editor while
// its tab shows another place (generator, import, ...), src/lib/passwords/draftStash.ts.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import { createDraftStash } from '../src/lib/passwords/draftStash.ts'

test('a kept draft comes back once, for the same tab and entry', () => {
  const stash = createDraftStash<string>()
  stash.keep('tab-1', 'e1', 'draft')
  assert.equal(stash.take('tab-1', 'e1'), 'draft')
  assert.equal(stash.take('tab-1', 'e1'), null)
})

test('another entry or another tab does not get it', () => {
  const stash = createDraftStash<string>()
  stash.keep('tab-1', 'e1', 'draft')
  assert.equal(stash.take('tab-1', 'e2'), null)
  assert.equal(stash.take('tab-2', 'e1'), null)
  assert.equal(stash.take('tab-1', 'e1'), 'draft')
})

test('a newer draft replaces the older one', () => {
  const stash = createDraftStash<string>()
  stash.keep('tab-1', 'new', 'old')
  stash.keep('tab-1', 'new', 'newer')
  assert.equal(stash.take('tab-1', 'new'), 'newer')
})

test('dropping a tab forgets all its drafts and only its', () => {
  const stash = createDraftStash<string>()
  stash.keep('tab-1', 'e1', 'a')
  stash.keep('tab-1', 'new', 'b')
  stash.keep('tab-2', 'e1', 'c')
  stash.dropTab('tab-1')
  assert.equal(stash.take('tab-1', 'e1'), null)
  assert.equal(stash.take('tab-1', 'new'), null)
  assert.equal(stash.take('tab-2', 'e1'), 'c')
})
