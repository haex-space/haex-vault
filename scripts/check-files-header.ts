// Part of `pnpm check:files` (spec 090-app-header-bar, T034): the file browser's header description
// and the commands its events stand for (`src/lib/files/header.ts`).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  filesHeader,
  filesHeaderCommand,
  inWindowHeader,
  type FilesFrameState,
  type HeaderKeyTarget,
  type PanelBarState,
} from '../src/lib/files/header.ts'
import type { WmHeaderControl } from '../src/types/bindings/WmHeaderControl.ts'
import type { WmHeaderDescription } from '../src/types/bindings/WmHeaderDescription.ts'

/** A translation that shows its key, so a test sees which text a control carries. */
const t = (key: string) => key

const FRAME: FilesFrameState = {
  sidebarVisible: true,
  splitAvailable: true,
  splitOn: false,
  splitDisabled: false,
}

const PANEL: PanelBarState = {
  path: '/home/anna/Dokumente',
  canGoUp: true,
  canGoBack: true,
  canGoForward: false,
  view: 'list',
  sort: { key: 'name', ascending: true },
  showHidden: false,
  canCreate: true,
  canCreateEncrypted: true,
}

const ids = (controls: readonly WmHeaderControl[] | undefined) =>
  (controls ?? []).map((c) => c.id)

const all = (d: WmHeaderDescription): WmHeaderControl[] => [
  ...(d.start ?? []),
  ...(d.center ?? []),
  ...(d.end ?? []),
]

const find = (d: WmHeaderDescription, id: string): WmHeaderControl => {
  const found = all(d).find((c) => c.id === id)
  assert.ok(found, `no control "${id}"`)
  return found
}

// ---------------------------------------------------------------------------
// One panel: the whole first row of spec 044
// ---------------------------------------------------------------------------

test('one panel: sidebar, back, forward, up, the path, reload, create, view, sort and the split switch', () => {
  const header = filesHeader(FRAME, PANEL, t)
  assert.deepEqual(ids(header.start), ['sidebar', 'back', 'forward', 'up'])
  assert.deepEqual(ids(header.center), ['path'])
  assert.deepEqual(ids(header.end), [
    'reload',
    'create',
    'view',
    'sort',
    'split',
  ])
})

test('the controls keep the test ids the scenarios use', () => {
  const header = filesHeader(FRAME, PANEL, t)
  const testIds = Object.fromEntries(
    all(header).map((c) => [c.id, (c as { testId?: string }).testId]),
  )
  assert.deepEqual(testIds, {
    sidebar: 'files-sidebar-toggle',
    back: 'files-back',
    forward: 'files-forward',
    up: 'files-up',
    path: 'files-path-bar',
    reload: 'files-reload',
    create: 'files-create',
    view: 'files-view',
    sort: 'files-sort',
    split: 'files-split-toggle',
  })
})

test('every control has a label, and the ids are unique', () => {
  const header = filesHeader(FRAME, PANEL, t)
  for (const control of all(header)) assert.ok(control.label, control.id)
  assert.equal(new Set(ids(all(header))).size, all(header).length)
})

test('back, forward and up are disabled when there is nowhere to go', () => {
  const header = filesHeader(
    FRAME,
    { ...PANEL, canGoUp: false, canGoBack: false, canGoForward: true },
    t,
  )
  assert.equal(find(header, 'up').disabled, true)
  assert.equal(find(header, 'back').disabled, true)
  assert.equal(find(header, 'forward').disabled, false)
})

test('the sidebar button is pressed while the sidebar shows, and says what it does', () => {
  const shown = find(filesHeader(FRAME, PANEL, t), 'sidebar')
  assert.equal((shown as { pressed: boolean }).pressed, true)
  assert.equal(shown.label, 'files.sidebar.hide')
  const hidden = find(
    filesHeader({ ...FRAME, sidebarVisible: false }, PANEL, t),
    'sidebar',
  )
  assert.equal((hidden as { pressed: boolean }).pressed, false)
  assert.equal(hidden.label, 'files.sidebar.show')
})

test('the path is the crumbs of the folder, the root under the storage name when it has one', () => {
  const crumbs = (d: WmHeaderDescription) =>
    (find(d, 'path') as { segments: { id: string; label: string }[] }).segments
  assert.deepEqual(
    crumbs(filesHeader(FRAME, PANEL, t)).map((s) => s.label),
    ['/', 'home', 'anna', 'Dokumente'],
  )
  assert.deepEqual(
    crumbs(
      filesHeader(FRAME, { ...PANEL, path: '/a', rootLabel: 'NAS' }, t),
    ).map((s) => s.label),
    ['NAS', 'a'],
  )
  const path = find(filesHeader(FRAME, PANEL, t), 'path') as {
    editable: boolean
    editValue: string
  }
  assert.equal(path.editable, true)
  assert.equal(path.editValue, '/home/anna/Dokumente')
})

test('a panel without a folder has no path bar', () => {
  const header = filesHeader(FRAME, { ...PANEL, path: null }, t)
  assert.deepEqual(ids(header.center), [])
})

test('create is disabled where nothing can be created, and the encrypted folder is offered only where it can be made', () => {
  const create = (panel: Partial<PanelBarState>) =>
    find(filesHeader(FRAME, { ...PANEL, ...panel }, t), 'create') as {
      disabled?: boolean
      items: { id?: string }[]
    }
  assert.equal(create({ canCreate: false }).disabled, true)
  assert.equal(create({}).disabled, false)
  assert.deepEqual(
    create({}).items.map((i) => i.id),
    ['folder', 'file', 'encrypted-folder'],
  )
  assert.deepEqual(
    create({ canCreateEncrypted: false }).items.map((i) => i.id),
    ['folder', 'file'],
  )
})

test('the view is a choice of list and grid with the panel’s view selected', () => {
  const view = (v: 'list' | 'grid') =>
    find(filesHeader(FRAME, { ...PANEL, view: v }, t), 'view') as {
      selected: string
      options: { id: string }[]
    }
  assert.deepEqual(
    view('list').options.map((o) => o.id),
    ['list', 'grid'],
  )
  assert.equal(view('list').selected, 'list')
  assert.equal(view('grid').selected, 'grid')
})

test('the sort menu marks the sort key and its direction, and ends with the hidden-files entry', () => {
  const sort = (panel: Partial<PanelBarState>) =>
    (
      find(filesHeader(FRAME, { ...PANEL, ...panel }, t), 'sort') as {
        items: {
          type: string
          id?: string
          icon?: string
          label?: string
          checked?: boolean
        }[]
      }
    ).items
  const asc = sort({ sort: { key: 'size', ascending: true } })
  assert.deepEqual(
    asc.filter((i) => i.type === 'item').map((i) => i.id),
    ['name', 'size', 'modified', 'type', 'hidden'],
  )
  assert.equal(asc.find((i) => i.id === 'size')?.icon, 'lucide:arrow-up')
  assert.equal(asc.find((i) => i.id === 'name')?.icon, 'lucide:minus')
  const desc = sort({ sort: { key: 'size', ascending: false } })
  assert.equal(desc.find((i) => i.id === 'size')?.icon, 'lucide:arrow-down')
  // The state is a check mark at the end of the row, not a changing label.
  assert.deepEqual(
    asc.filter((i) => i.checked).map((i) => i.id),
    ['size'],
  )
  assert.equal(asc.find((i) => i.id === 'hidden')?.label, 'files.hidden.show')
  assert.equal(asc.find((i) => i.id === 'hidden')?.checked, false)
  assert.equal(
    sort({ showHidden: true }).find((i) => i.id === 'hidden')?.checked,
    true,
  )
  assert.equal(asc.at(-2)?.type, 'separator')
})

// ---------------------------------------------------------------------------
// Collapse order and forms
// ---------------------------------------------------------------------------

test('the narrower the window, the sooner the less used controls leave; the sidebar button stays', () => {
  const header = filesHeader(FRAME, PANEL, t)
  const order = (id: string) =>
    (find(header, id) as { collapse?: { order?: number } }).collapse?.order ??
    Infinity
  const leaving = [
    'view',
    'path',
    'sort',
    'create',
    'reload',
    'up',
    'forward',
    'back',
    'split',
  ]
  const orders = leaving.map(order)
  assert.deepEqual(
    orders,
    [...orders].sort((a, b) => a - b),
    'collapse order is not by use',
  )
  assert.equal(
    new Set(orders).size,
    orders.length,
    'two controls share an order',
  )
  assert.equal(
    (find(header, 'sidebar') as { collapse?: { form?: string } }).collapse
      ?.form,
    'keep',
  )
  assert.equal(
    (find(header, 'path') as { collapse?: { form?: string } }).collapse?.form,
    'shrink',
  )
})

// ---------------------------------------------------------------------------
// Two panels, a title, no split switch
// ---------------------------------------------------------------------------

test('two panels: the header keeps only the sidebar button and the split switch', () => {
  const header = filesHeader({ ...FRAME, splitOn: true }, null, t)
  assert.deepEqual(ids(all(header)), ['sidebar', 'split'])
  assert.equal((find(header, 'split') as { pressed: boolean }).pressed, true)
})

test('the split switch is disabled when the window is too narrow, and says why', () => {
  const split = find(
    filesHeader({ ...FRAME, splitDisabled: true }, PANEL, t),
    'split',
  )
  assert.equal(split.disabled, true)
  assert.equal(split.tooltip, 'files.split.tooNarrow')
  assert.equal(
    find(filesHeader(FRAME, PANEL, t), 'split').tooltip,
    'files.split.toggle',
  )
})

test('without the switch (a phone) there is none', () => {
  const header = filesHeader({ ...FRAME, splitAvailable: false }, PANEL, t)
  assert.ok(!ids(all(header)).includes('split'))
})

test('a panel that shows a list has back, forward and its title, nothing for a folder', () => {
  const header = filesHeader(FRAME, { ...PANEL, title: 'Sync rules' }, t)
  assert.deepEqual(ids(header.start), ['sidebar', 'back', 'forward'])
  assert.deepEqual(ids(header.center), ['title'])
  assert.deepEqual(ids(header.end), ['split'])
  assert.equal((find(header, 'title') as { text: string }).text, 'Sync rules')
})

// ---------------------------------------------------------------------------
// Events → commands
// ---------------------------------------------------------------------------

const cmd = (
  id: string,
  kind: string,
  value?: string | boolean,
  panel: PanelBarState | null = PANEL,
) => filesHeaderCommand({ id, kind, value } as never, panel)

test('the frame’s own controls work with two panels too', () => {
  assert.deepEqual(cmd('sidebar', 'toggle', false, null), { kind: 'sidebar' })
  assert.deepEqual(cmd('split', 'toggle', true, null), { kind: 'split' })
  assert.equal(cmd('back', 'click', undefined, null), null)
})

test('back, forward, up and reload are clicks', () => {
  for (const id of ['back', 'forward', 'up', 'reload'] as const)
    assert.deepEqual(cmd(id, 'click'), { kind: id })
  assert.equal(cmd('back', 'toggle', true), null)
})

test('a crumb goes to its folder, a typed path goes where it says', () => {
  assert.deepEqual(cmd('path', 'select', '1'), { kind: 'go', path: '/home' })
  assert.deepEqual(cmd('path', 'select', '3'), {
    kind: 'go',
    path: '/home/anna/Dokumente',
  })
  assert.deepEqual(cmd('path', 'submit', '  /tmp  '), {
    kind: 'go',
    path: '/tmp',
  })
  assert.equal(cmd('path', 'submit', '   '), null)
  assert.equal(cmd('path', 'select', '99'), null)
  assert.equal(cmd('path', 'select', 'x'), null)
})

test('the create menu entries map to the create commands', () => {
  assert.deepEqual(cmd('create', 'click', 'folder'), {
    kind: 'create',
    command: 'newFolder',
  })
  assert.deepEqual(cmd('create', 'click', 'file'), {
    kind: 'create',
    command: 'newFile',
  })
  assert.deepEqual(cmd('create', 'click', 'encrypted-folder'), {
    kind: 'create',
    command: 'newEncryptedFolder',
  })
  assert.equal(cmd('create', 'click', 'other'), null)
})

test('the view choice sets the view', () => {
  assert.deepEqual(cmd('view', 'select', 'grid'), {
    kind: 'view',
    view: 'grid',
  })
  assert.equal(cmd('view', 'select', 'tiles'), null)
})

test('a sort entry sets the key, the same key again flips the direction; the hidden entry flips the filter', () => {
  assert.deepEqual(cmd('sort', 'click', 'size'), {
    kind: 'sort',
    sort: { key: 'size', ascending: true },
  })
  assert.deepEqual(cmd('sort', 'click', 'name'), {
    kind: 'sort',
    sort: { key: 'name', ascending: false },
  })
  assert.deepEqual(cmd('sort', 'click', 'hidden'), {
    kind: 'hidden',
    show: true,
  })
  assert.deepEqual(
    filesHeaderCommand(
      { id: 'sort', kind: 'click', value: 'hidden' },
      { ...PANEL, showHidden: true },
    ),
    { kind: 'hidden', show: false },
  )
  assert.equal(cmd('sort', 'click', 'colour'), null)
})

test('an event of a control that is not there asks for nothing', () => {
  assert.equal(cmd('nope', 'click'), null)
})

// ---------------------------------------------------------------------------
// Keys of the header's controls
// ---------------------------------------------------------------------------

/** A stand-in for a DOM element: `closest` finds the nearest ancestor (or itself) that matches. */
function node(
  attributes: Record<string, string>,
  parent: HeaderKeyTarget | null = null,
): HeaderKeyTarget {
  const self: HeaderKeyTarget = {
    getAttribute: (name) => attributes[name] ?? null,
    closest(selector) {
      const matches =
        selector === '[data-testid="wm-header"]'
          ? attributes['data-testid'] === 'wm-header'
          : selector === '[data-wm-window-id]'
            ? 'data-wm-window-id' in attributes
            : false
      return matches ? self : (parent?.closest(selector) ?? null)
    },
  }
  return self
}

test('a control of the header of this window is a header key target', () => {
  const window = node({ 'data-wm-window-id': 'w1' })
  const header = node({ 'data-testid': 'wm-header' }, window)
  const button = node({ 'data-testid': 'files-back' }, header)
  assert.equal(inWindowHeader(button, 'w1'), true)
})

test('the header of another window, a control outside a header and no target are not', () => {
  const window = node({ 'data-wm-window-id': 'w2' })
  const header = node({ 'data-testid': 'wm-header' }, window)
  assert.equal(inWindowHeader(node({}, header), 'w1'), false)
  const body = node({ 'data-testid': 'files-sidebar' }, window)
  assert.equal(inWindowHeader(body, 'w2'), false)
  assert.equal(inWindowHeader(null, 'w2'), false)
})
