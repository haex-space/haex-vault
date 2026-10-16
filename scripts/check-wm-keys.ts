// Part of `pnpm check:wm-navigation` (spec 020-tab-navigation): the keyboard shortcuts of
// window manager actions (keybindings.ts) — chord normalization, platform detection, chord resolution
// against the shipped catalog and yielding to focused text fields (FR-017). Split from
// check-wm-actions.ts to stay below the 500-line limit.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import { ALL_ACTIONS } from '../src/lib/actions/catalog.ts'
import {
  chordFromEvent,
  detectPlatform,
  isEditableTarget,
  resolveChord,
  shouldYield,
} from '../src/lib/wm/keybindings.ts'
import { shortcutLabel } from '../src/lib/wm/shortcutLabel.ts'

// ---------------------------------------------------------------------------
// Keybindings (keybindings.ts)
// ---------------------------------------------------------------------------

function key(
  code: string,
  mods: Partial<Record<'ctrl' | 'alt' | 'shift' | 'meta', boolean>> = {},
) {
  return {
    code,
    ctrlKey: mods.ctrl ?? false,
    altKey: mods.alt ?? false,
    shiftKey: mods.shift ?? false,
    metaKey: mods.meta ?? false,
  }
}

test('chordFromEvent normalizes modifiers in fixed order', () => {
  assert.equal(chordFromEvent(key('ArrowLeft', { alt: true })), 'Alt+ArrowLeft')
  assert.equal(
    chordFromEvent(
      key('KeyT', { meta: true, shift: true, ctrl: true, alt: true }),
    ),
    'Ctrl+Alt+Shift+Meta+KeyT',
  )
  assert.equal(chordFromEvent(key('AltLeft', { alt: true })), null)
})

test('detectPlatform prefers userAgentData and recognizes macOS', () => {
  assert.equal(detectPlatform({ platform: 'MacIntel' }), 'mac')
  assert.equal(detectPlatform({ platform: 'Linux x86_64' }), 'default')
  assert.equal(
    detectPlatform({ platform: 'Win32', userAgentData: { platform: 'macOS' } }),
    'mac',
  )
})

test('Alt+Arrow resolves to back/forward everywhere, Cmd+[ / Cmd+] only on mac', () => {
  assert.equal(
    resolveChord(ALL_ACTIONS, 'Alt+ArrowLeft', 'default')?.id,
    'wm.tab.back',
  )
  assert.equal(
    resolveChord(ALL_ACTIONS, 'Alt+ArrowRight', 'mac')?.id,
    'wm.tab.forward',
  )
  assert.equal(
    resolveChord(ALL_ACTIONS, 'Meta+BracketLeft', 'mac')?.id,
    'wm.tab.back',
  )
  assert.equal(
    resolveChord(ALL_ACTIONS, 'Meta+BracketLeft', 'default'),
    undefined,
  )
  assert.equal(resolveChord(ALL_ACTIONS, 'Ctrl+KeyZ', 'default'), undefined)
})

test('on mac Alt+Arrow yields to a focused text field; elsewhere it navigates (FR-017)', () => {
  const back = resolveChord(ALL_ACTIONS, 'Alt+ArrowLeft', 'mac')
  assert.ok(back)
  const textarea = { tagName: 'TEXTAREA' }
  const button = { tagName: 'BUTTON' }
  assert.equal(shouldYield(back, 'mac', 'Alt+ArrowLeft', textarea), true)
  assert.equal(shouldYield(back, 'mac', 'Alt+ArrowLeft', button), false)
  assert.equal(shouldYield(back, 'mac', 'Meta+BracketLeft', textarea), false)
  assert.equal(shouldYield(back, 'default', 'Alt+ArrowLeft', textarea), false)
})

test('isEditableTarget recognizes text inputs, textareas and contenteditable', () => {
  assert.ok(isEditableTarget({ tagName: 'INPUT', type: 'text' }))
  assert.ok(isEditableTarget({ tagName: 'INPUT', type: 'search' }))
  assert.ok(!isEditableTarget({ tagName: 'INPUT', type: 'checkbox' }))
  assert.ok(isEditableTarget({ tagName: 'TEXTAREA' }))
  assert.ok(isEditableTarget({ tagName: 'DIV', isContentEditable: true }))
  assert.ok(!isEditableTarget({ tagName: 'DIV' }))
  assert.ok(!isEditableTarget(null))
})

// Spec 036 FR-016, spec 044 FR-017b: the label next to a menu action, shared by every app.
test('the label of a shortcut follows the platform and the language', () => {
  const de = {
    mod: 'Strg',
    shift: 'Umschalt',
    Delete: 'Entf',
    Enter: 'Eingabe',
  }
  assert.equal(shortcutLabel({ mod: true, key: 'X' }, 'default', de), 'Strg+X')
  assert.equal(
    shortcutLabel({ mod: true, shift: true, key: 'C' }, 'default', de),
    'Strg+Umschalt+C',
  )
  assert.equal(shortcutLabel({ key: 'Delete' }, 'default', de), 'Entf')
  assert.equal(shortcutLabel({ mod: true, key: 'X' }, 'mac', de), '⌘X')
  assert.equal(
    shortcutLabel({ mod: true, shift: true, key: 'C' }, 'mac', de),
    '⇧⌘C',
  )
  assert.equal(shortcutLabel({ key: 'Enter' }, 'mac', de), '↩')
  assert.equal(shortcutLabel({ key: 'Delete' }, 'mac', de), '⌫')
  assert.equal(shortcutLabel({ key: 'F2' }, 'default', de), 'F2')
  assert.equal(
    shortcutLabel({ mod: true, shift: true, key: 'N' }, 'mac', de),
    '⇧⌘N',
  )
})
