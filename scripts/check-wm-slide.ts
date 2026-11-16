// Part of `pnpm check:wm-state` (spec 090-app-header-bar): the one slide definition of the app
// sidebars, the header's overflow sidebar and its search field (`src/lib/wm/slide.ts`). Every panel
// stands still for a reduced-motion setting, the sidebar keeps sliding in from the left and hiding
// by its width, the overflow sidebar slides in from the right like it, and the search field drops
// in below the header.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

import {
  DROP_FROM_TOP,
  SLIDE_TIMING,
  sidebarPanelClass,
} from '../src/lib/wm/slide.ts'

test('a narrow sidebar slides in from the left and out again', () => {
  const open = sidebarPanelClass(true, false).join(' ')
  const closed = sidebarPanelClass(false, false).join(' ')
  assert.match(open, /(^| )visible translate-x-0( |$)/)
  assert.match(closed, /(^| )invisible -translate-x-full( |$)/)
  assert.match(closed, /transition-\[translate,width,visibility\]/)
})

test('a wide sidebar is hidden by its width and has one width in every app', () => {
  assert.match(
    sidebarPanelClass(false, true).join(' '),
    /@2xl:invisible @2xl:w-0/,
  )
  assert.match(
    sidebarPanelClass(false, false).join(' '),
    /@2xl:visible @2xl:w-64/,
  )
})

test('every slide stands still for reduced motion', () => {
  assert.match(SLIDE_TIMING, /motion-reduce:transition-none/)
  assert.match(
    sidebarPanelClass(false, false).join(' '),
    /motion-reduce:transition-none/,
  )
  for (const set of [DROP_FROM_TOP]) {
    assert.match(set.enterActiveClass, /motion-reduce:transition-none/)
    assert.match(set.leaveActiveClass, /motion-reduce:transition-none/)
  }
})

test('the overflow sidebar is the sidebar from the other side, the search field drops in from above', () => {
  const classes = (side?: 'right') =>
    sidebarPanelClass(false, false, side).filter(Boolean)
  assert.match(
    classes('right').join(' '),
    /(^| )invisible translate-x-full( |$)/,
  )
  assert.match(
    sidebarPanelClass(true, false, 'right').join(' '),
    /(^| )visible translate-x-0( |$)/,
  )
  // Same width, same slide and the same wide column; only the side and its border differ.
  const common = (list: string[]) =>
    list.filter(
      (c) => c !== 'border-l border-border' && !c.includes('translate-x-full'),
    )
  assert.ok(classes('right').includes('border-l border-border'))
  assert.deepEqual(common(classes('right')), common(classes()))
  assert.match(DROP_FROM_TOP.enterFromClass, /(^| )opacity-0( |$)/)
  assert.match(DROP_FROM_TOP.enterFromClass, /(^| )-translate-y-2( |$)/)
  assert.equal(DROP_FROM_TOP.enterFromClass, DROP_FROM_TOP.leaveToClass)
})

// Every app frame (and the chat's) has its sidebar from `WmSidebar`; none draws its own panel again.
const FRAMES = [
  'apps/ContactsApp.vue',
  'apps/FilesApp.vue',
  'apps/PasswordsApp.vue',
  'apps/SettingsApp.vue',
  'apps/WalletApp.vue',
  'chat/SidebarLayout.vue',
  'wm/HeaderOverflow.vue',
]
const source = (path: string) =>
  readFileSync(new URL(`../src/components/${path}`, import.meta.url), 'utf8')

test('every app frame uses the shared sidebar and has no slide classes of its own', () => {
  for (const frame of FRAMES) {
    const text = source(frame)
    assert.match(text, /<WmSidebar\b/, `${frame} uses WmSidebar`)
    assert.doesNotMatch(text, /-translate-x-full/, `${frame} slides on its own`)
  }
})
