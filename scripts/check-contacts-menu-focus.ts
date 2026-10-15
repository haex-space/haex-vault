// Part of `pnpm check:contacts` (spec 051): the focus after the menu that added a value closes
// (src/lib/contacts/menuFocus.ts). e2e `contacts-basic` lost typed keys to the label field when the
// menu's close came after the first keys of the value had been typed.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import { focusAfterMenu } from '../src/lib/contacts/menuFocus.ts'

test('a field of the form that holds the focus keeps it', () => {
  assert.equal(focusAfterMenu({ inForm: true, editable: true }), 'keep')
})

test('the new value takes the focus from the menu, the button or the page', () => {
  assert.equal(focusAfterMenu({ inForm: false, editable: false }), 'new-value')
  assert.equal(focusAfterMenu({ inForm: false, editable: true }), 'new-value')
  assert.equal(focusAfterMenu({ inForm: true, editable: false }), 'new-value')
  assert.equal(focusAfterMenu(null), 'new-value')
})
