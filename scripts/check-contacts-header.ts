// Part of `pnpm check:contacts` (spec 093-app-headers, US2): the address book's header description
// and the commands its events stand for (`src/lib/contacts/header.ts`).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  contactsHeader,
  contactsHeaderCommand,
  type ContactsHeaderState,
} from '../src/lib/contacts/header.ts'
import { headerProblems } from '../src/lib/wm/headerChecks.ts'
import type { WmHeaderControl } from '../src/types/bindings/WmHeaderControl.ts'

/** A translation that shows its key, so a test sees which text a control carries. */
const t = (key: string) => key

const STATE: ContactsHeaderState = {
  sidebarVisible: true,
  entryOpen: false,
  query: '',
}

const ids = (controls: readonly WmHeaderControl[] | undefined) =>
  (controls ?? []).map((c) => c.id)
const find = (state: ContactsHeaderState, id: string) => {
  const d = contactsHeader(state, t)
  return [...(d.start ?? []), ...(d.center ?? []), ...(d.end ?? [])].find(
    (c) => c.id === id,
  )
}

test('a list shows the sidebar button, the search and "+"', () => {
  const d = contactsHeader(STATE, t)
  assert.deepEqual(ids(d.start), ['sidebar'])
  assert.deepEqual(ids(d.center), ['q'])
  assert.deepEqual(ids(d.end), ['add'])
})

test('an open contact or a share being read keeps the sidebar button only', () => {
  const d = contactsHeader({ ...STATE, entryOpen: true }, t)
  assert.deepEqual(ids(d.start), ['sidebar'])
  assert.deepEqual(ids(d.center), [])
  assert.deepEqual(ids(d.end), [])
})

test('the sidebar button is pressed while the sidebar shows, and never collapses', () => {
  const on = find(STATE, 'sidebar')
  assert.ok(on?.type === 'toggle')
  assert.equal(on.pressed, true)
  assert.equal(on.label, 'contacts.sidebar.hide')
  assert.deepEqual(on.collapse, { form: 'keep' })
  const off = find({ ...STATE, sidebarVisible: false }, 'sidebar')
  assert.ok(off?.type === 'toggle')
  assert.equal(off.label, 'contacts.sidebar.show')
})

test('the search carries the text and gives way before "+"', () => {
  const q = find({ ...STATE, query: 'beck' }, 'q')
  assert.ok(q?.type === 'search')
  assert.equal(q.value, 'beck')
  assert.deepEqual(q.collapse, { order: 10, form: 'shrink' })
  const add = find(STATE, 'add')
  assert.ok((add?.collapse?.order ?? 1001) > 10)
})

test('the test ids of today stay: controls by name, entries as <menu>-<entry>', () => {
  assert.equal(find(STATE, 'sidebar')?.testId, 'contacts-sidebar-toggle')
  assert.equal(find(STATE, 'q')?.testId, 'contacts-search')
  const add = find(STATE, 'add')
  assert.equal(add?.testId, 'contacts-add')
  assert.ok(add?.type === 'menu')
  assert.deepEqual(
    add.items.flatMap((i) => (i.type === 'item' ? [i.id] : [])),
    ['new', 'import'],
  )
})

test('the description passes the header checks', () => {
  assert.deepEqual(headerProblems(contactsHeader(STATE, t)), [])
})

test('events stand for the commands the toolbar had', () => {
  const cmd = contactsHeaderCommand
  assert.deepEqual(cmd({ id: 'sidebar', kind: 'toggle', value: false }), {
    kind: 'sidebar',
  })
  assert.deepEqual(cmd({ id: 'q', kind: 'input', value: 'beck' }), {
    kind: 'search',
    text: 'beck',
  })
  assert.equal(cmd({ id: 'q', kind: 'submit', value: 'beck' }), null)
  assert.deepEqual(cmd({ id: 'add', kind: 'click', value: 'new' }), {
    kind: 'newContact',
  })
  assert.deepEqual(cmd({ id: 'add', kind: 'click', value: 'import' }), {
    kind: 'import',
  })
  assert.equal(cmd({ id: 'add', kind: 'click', value: 'zzz' }), null)
  assert.equal(cmd({ id: 'unknown', kind: 'click' }), null)
})
