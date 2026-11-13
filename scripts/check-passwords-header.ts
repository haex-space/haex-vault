// Part of `pnpm check:passwords` (spec 093-app-headers, US1): the password manager's header
// description and the commands its events stand for (`src/lib/passwords/header.ts`).
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

import {
  passwordsHeader,
  passwordsHeaderCommand,
  type PasswordsHeaderState,
} from '../src/lib/passwords/header.ts'
import { headerProblems } from '../src/lib/wm/headerChecks.ts'
import {
  focusHeaderControl,
  headerControl,
  type HeaderSource,
} from '../src/lib/wm/headerDom.ts'
import type { WmHeaderControl } from '../src/types/bindings/WmHeaderControl.ts'

/** A translation that shows its key, so a test sees which text a control carries. */
const t = (key: string) => key

const STATE: PasswordsHeaderState = {
  sidebarVisible: true,
  entryOpen: false,
  query: '',
  tags: [
    { id: 'work', name: 'Work', itemCount: 4 },
    { id: 'home', name: 'Home', itemCount: 2 },
  ],
  activeTagId: null,
}

const ids = (controls: readonly WmHeaderControl[] | undefined) =>
  (controls ?? []).map((c) => c.id)
const find = (state: PasswordsHeaderState, id: string) => {
  const d = passwordsHeader(state, t)
  return [...(d.start ?? []), ...(d.center ?? []), ...(d.end ?? [])].find(
    (c) => c.id === id,
  )
}

test('a list shows the sidebar button, the search, the tag filter, "+" and the menu', () => {
  const d = passwordsHeader(STATE, t)
  assert.deepEqual(ids(d.start), ['sidebar'])
  assert.deepEqual(ids(d.center), ['q'])
  assert.deepEqual(ids(d.end), ['tag-filter', 'new', 'menu'])
})

test('an open entry keeps the sidebar button and the menu only', () => {
  const d = passwordsHeader({ ...STATE, entryOpen: true }, t)
  assert.deepEqual(ids(d.start), ['sidebar'])
  assert.deepEqual(ids(d.center), [])
  assert.deepEqual(ids(d.end), ['menu'])
})

test('the tag filter is left out while there is no tag', () => {
  const d = passwordsHeader({ ...STATE, tags: [] }, t)
  assert.deepEqual(ids(d.end), ['new', 'menu'])
})

test('the sidebar button is pressed while the sidebar shows, and never collapses', () => {
  const on = find(STATE, 'sidebar')
  assert.ok(on?.type === 'toggle')
  assert.equal(on.pressed, true)
  assert.equal(on.label, 'passwords.sidebar.hide')
  assert.deepEqual(on.collapse, { form: 'keep' })
  const off = find({ ...STATE, sidebarVisible: false }, 'sidebar')
  assert.ok(off?.type === 'toggle')
  assert.equal(off.pressed, false)
  assert.equal(off.label, 'passwords.sidebar.show')
})

test('the search carries the text and is the first control to give way', () => {
  const q = find({ ...STATE, query: 'git' }, 'q')
  assert.ok(q?.type === 'search')
  assert.equal(q.value, 'git')
  assert.deepEqual(q.collapse, { order: 10, form: 'shrink' })
})

test('the tag filter marks "all tags" or the picked tag, and is active only with a tag', () => {
  const none = find(STATE, 'tag-filter')
  assert.ok(none?.type === 'menu')
  assert.notEqual(none.active, true)
  const checked = (c: WmHeaderControl) =>
    c.type === 'menu'
      ? c.items.flatMap((i) => (i.type === 'item' && i.checked ? [i.id] : []))
      : []
  assert.deepEqual(checked(none), ['all'])

  const picked = find({ ...STATE, activeTagId: 'home' }, 'tag-filter')
  assert.ok(picked?.type === 'menu')
  assert.equal(picked.active, true)
  assert.deepEqual(checked(picked), ['home'])
})

test('a tag entry shows its name and its count', () => {
  const filter = find(STATE, 'tag-filter')
  assert.ok(filter?.type === 'menu')
  const work = filter.items.find((i) => i.type === 'item' && i.id === 'work')
  assert.ok(work?.type === 'item')
  assert.equal(work.label, 'Work (4)')
})

test('the test ids of today stay: controls by name, entries as <menu>-<entry>', () => {
  const testIds = (id: string) => find(STATE, id)?.testId
  assert.equal(testIds('sidebar'), 'passwords-sidebar-toggle')
  assert.equal(testIds('q'), 'passwords-search')
  assert.equal(testIds('tag-filter'), 'passwords-tag-filter')
  assert.equal(testIds('new'), 'passwords-new')
  assert.equal(testIds('menu'), 'passwords-menu')
  const entries = (id: string) => {
    const c = find(STATE, id)
    return c?.type === 'menu'
      ? c.items.flatMap((i) => (i.type === 'item' ? [i.id] : []))
      : []
  }
  assert.deepEqual(entries('new'), ['entry', 'folder'])
  assert.deepEqual(entries('menu'), ['generator', 'import', 'tags', 'settings'])
  assert.equal(entries('tag-filter')[0], 'all')
})

test('"+" and the menu leave the row after the filter, and "+" leaves last', () => {
  const order = (id: string) => find(STATE, id)?.collapse?.order ?? 1001
  assert.ok(order('q') < order('tag-filter'))
  assert.ok(order('tag-filter') < order('menu'))
  assert.ok(order('menu') < order('new'))
})

test('the description passes the header checks', () => {
  assert.deepEqual(headerProblems(passwordsHeader(STATE, t)), [])
  assert.deepEqual(
    headerProblems(passwordsHeader({ ...STATE, activeTagId: 'work' }, t)),
    [],
  )
})

test('events stand for the commands the toolbar had', () => {
  const cmd = passwordsHeaderCommand
  assert.deepEqual(cmd({ id: 'sidebar', kind: 'toggle', value: false }), {
    kind: 'sidebar',
  })
  assert.deepEqual(cmd({ id: 'q', kind: 'input', value: 'git' }), {
    kind: 'search',
    text: 'git',
  })
  assert.equal(cmd({ id: 'q', kind: 'submit', value: 'git' }), null)
  assert.deepEqual(cmd({ id: 'tag-filter', kind: 'click', value: 'all' }), {
    kind: 'tag',
    id: null,
  })
  assert.deepEqual(cmd({ id: 'tag-filter', kind: 'click', value: 'work' }), {
    kind: 'tag',
    id: 'work',
  })
  assert.deepEqual(cmd({ id: 'new', kind: 'click', value: 'entry' }), {
    kind: 'newEntry',
  })
  assert.deepEqual(cmd({ id: 'new', kind: 'click', value: 'folder' }), {
    kind: 'newFolder',
  })
  for (const entry of ['generator', 'import', 'tags', 'settings'] as const)
    assert.deepEqual(cmd({ id: 'menu', kind: 'click', value: entry }), {
      kind: entry,
    })
  assert.equal(cmd({ id: 'menu', kind: 'click', value: 'zzz' }), null)
  assert.equal(cmd({ id: 'unknown', kind: 'click' }), null)
})

// The keyboard (spec 093, "Keyboard", T013a): the field of the search is in the window's header,
// outside the app's root, or in the overflow panel while it has collapsed there.

/** A window of stubs: the header's row and the overflow panel hold the controls by selector. */
function stubWindow(where: {
  row?: boolean
  panel?: boolean
  overflow?: boolean
}) {
  const focused: string[] = []
  const field = (name: string) => ({ focus: () => focused.push(name) })
  const selectors: Record<string, unknown> = {
    '[data-testid="wm-header"] [data-testid="passwords-search"]': where.row
      ? field('row')
      : null,
    '[data-testid="wm-header-overflow-menu"] [data-testid="passwords-search"]':
      where.panel ? field('panel') : null,
    '[data-testid="wm-header"] [data-testid="wm-header-overflow"]':
      where.overflow ? field('button') : null,
  }
  const frame = {
    closest: () => null,
    querySelector: (selector: string) => selectors[selector] ?? null,
  }
  const area = {
    closest: (selector: string) =>
      selector === '[data-wm-window-id]' ? frame : null,
    querySelector: () => null,
  }
  return { area: area as HeaderSource, focused }
}

test('the search in the header row is reached from the app and takes the focus', () => {
  const { area, focused } = stubWindow({ row: true })
  assert.ok(headerControl(area, 'passwords-search'))
  let opened = 0
  assert.equal(
    focusHeaderControl(
      area,
      'passwords-search',
      () => opened++,
      async () => {},
    ),
    true,
  )
  assert.deepEqual(focused, ['row'])
  assert.equal(opened, 0)
})

test('a collapsed search opens the overflow panel first, then takes the focus', async () => {
  const { area, focused } = stubWindow({ panel: true, overflow: true })
  assert.equal(headerControl(area, 'passwords-search'), null)
  let opened = 0
  let render = () => {}
  const afterRender = () => new Promise<void>((resolve) => (render = resolve))
  assert.equal(
    focusHeaderControl(area, 'passwords-search', () => opened++, afterRender),
    true,
  )
  assert.equal(opened, 1)
  assert.deepEqual(focused, [], 'the panel is not drawn yet')
  render()
  await Promise.resolve()
  await Promise.resolve()
  assert.deepEqual(focused, ['panel'])
})

test('without a window manager or a window there is no panel to open and no key is taken', () => {
  let opened = 0
  const bare = stubWindow({ overflow: true })
  assert.equal(
    focusHeaderControl(bare.area, 'passwords-search', null, async () => {}),
    false,
  )
  assert.equal(
    focusHeaderControl(
      null,
      'passwords-search',
      () => opened++,
      async () => {},
    ),
    false,
  )
  const none = stubWindow({})
  assert.equal(
    focusHeaderControl(
      none.area,
      'passwords-search',
      () => opened++,
      async () => {},
    ),
    false,
  )
  assert.equal(opened, 0)
})

test('every text of the header exists in German and English (the menu once showed its key)', () => {
  const used = new Set<string>()
  const spy = (key: string) => {
    used.add(key)
    return key
  }
  for (const state of [
    STATE,
    { ...STATE, entryOpen: true },
    { ...STATE, sidebarVisible: false },
  ])
    passwordsHeader(state, spy)
  for (const language of ['de', 'en']) {
    const messages: unknown = JSON.parse(
      readFileSync(
        new URL(`../src/i18n/locales/${language}.json`, import.meta.url),
        'utf8',
      ),
    )
    for (const key of used) {
      const text = key
        .split('.')
        .reduce<unknown>(
          (node, part) =>
            node && typeof node === 'object'
              ? (node as Record<string, unknown>)[part]
              : undefined,
          messages,
        )
      assert.equal(typeof text, 'string', `${language}: "${key}" is no text`)
    }
  }
})
