// Part of `pnpm check:wm-state` (spec 090-app-header-bar): the one slide definition of the app
// sidebars and the header's overflow panel (`src/lib/wm/slide.ts`). Every panel stands still for a
// reduced-motion setting, the sidebar keeps sliding in from the left and hiding by its width, and
// the overflow panel slides from the right while its backdrop only fades.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

import {
  FADE,
  SLIDE_FROM_RIGHT,
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
  for (const set of [SLIDE_FROM_RIGHT, FADE]) {
    assert.match(set.enterActiveClass, /motion-reduce:transition-none/)
    assert.match(set.leaveActiveClass, /motion-reduce:transition-none/)
  }
})

test('the overflow panel moves from the right and its backdrop fades without moving', () => {
  assert.equal(SLIDE_FROM_RIGHT.enterFromClass, 'translate-x-full')
  assert.equal(SLIDE_FROM_RIGHT.leaveToClass, 'translate-x-full')
  assert.equal(FADE.enterFromClass, 'opacity-0')
  assert.equal(FADE.leaveToClass, 'opacity-0')
  assert.doesNotMatch(JSON.stringify(FADE), /translate/)
})

// Every app frame has its sidebar from `WmSidebar`; none draws its own panel again.
const FRAMES = [
  'apps/ContactsApp.vue',
  'apps/FilesApp.vue',
  'apps/PasswordsApp.vue',
  'apps/SettingsApp.vue',
  'apps/WalletApp.vue',
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
