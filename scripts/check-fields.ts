// Run with `node --test scripts/check-fields.ts`. Spec 035-appearance-and-fields, FR-001 and FR-010:
// every text field in `src/` is haex-ui's `UiInput`, `UiInputPassword`, `UiTextarea` or `UiSelect`.
// The older kinds (`ShadcnInput`, `ShadcnTextarea`, `ShadcnSelect`, and a bare `<input>`,
// `<textarea>` or `<select>`) are only allowed in the files listed in ALLOWED, each with its reason.
//
// The templates are parsed with the SFC compiler, so a word in a comment or a string never counts.
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import assert from 'node:assert/strict'
import { parse } from 'vue/compiler-sfc'

/** Files that may keep an older field kind, with the reason (FR-001). */
const ALLOWED: Record<string, string> = {
  'src/components/chat/ComposerShell.vue':
    'the chat input (agent and contact conversations) grows with its content and has its own key handling (Enter sends)',
  'src/components/chat/ComposerSettingsPopover.vue':
    'the model select has grouped options and a search field inside its list, with its own focus handling; UiSelect only takes a flat option list. Remove when haex-ui UiSelect can do this (tasks T003)',
  'src/components/passwords/ColorSelect.vue':
    'every option carries a colour dot (and the closed select shows it too); UiSelect only takes a flat list of text options. Remove when haex-ui UiSelect can do this (tasks T003)',
  'src/components/settings/Select.vue':
    'grouped options, an empty value and data-value for the e2e checks; the row title is its label. Remove when haex-ui UiSelect can do this (tasks T003)',
}

/** `<input type=...>` that is not a text field. */
const NON_TEXT_INPUT = new Set([
  'radio',
  'checkbox',
  'range',
  'file',
  'color',
  'hidden',
  'button',
  'submit',
  'reset',
])

type Hit = { tag: string; line: number }

// The compiler's node shapes are structural; only what is read here is typed.
type Node = {
  type: number
  tag?: string
  props?: Prop[]
  children?: Node[]
  loc: { start: { line: number } }
}
type Prop = {
  type: number
  name?: string
  value?: { content: string }
  arg?: { content?: string }
}

const OLD_COMPONENT =
  /^(Shadcn(Input|Textarea|Select)|shadcn-(input|textarea|select))$/

function isOldField(node: Node): boolean {
  const tag = node.tag ?? ''
  if (OLD_COMPONENT.test(tag)) return true
  if (tag === 'textarea' || tag === 'select') return true
  if (tag !== 'input') return false
  for (const prop of node.props ?? []) {
    // 6 = attribute, 7 = directive (`:type="..."` cannot be checked, so it counts as a text field)
    if (prop.type === 6 && prop.name === 'type') {
      return !NON_TEXT_INPUT.has(prop.value?.content ?? '')
    }
  }
  return true
}

/** The older field kinds in one `.vue` source, by tag and line. */
export function findOldFields(source: string, filename = 'x.vue'): Hit[] {
  const ast = parse(source, { filename }).descriptor.template?.ast as
    Node | undefined
  const hits: Hit[] = []
  const walk = (node: Node) => {
    if (node.type === 1 && isOldField(node)) {
      hits.push({ tag: node.tag ?? '', line: node.loc.start.line })
    }
    for (const child of node.children ?? []) walk(child)
  }
  if (ast) walk(ast)
  return hits
}

test('the scanner finds the older field kinds and skips what is not a text field', () => {
  const sample = (template: string) => `<template>${template}</template>`
  const tags = (template: string) =>
    findOldFields(sample(template)).map((hit) => hit.tag)

  assert.deepEqual(tags('<ShadcnInput v-model="a" />'), ['ShadcnInput'])
  assert.deepEqual(tags('<ShadcnTextarea />'), ['ShadcnTextarea'])
  assert.deepEqual(
    tags('<ShadcnSelect><ShadcnSelectTrigger /></ShadcnSelect>'),
    ['ShadcnSelect'],
  )
  assert.deepEqual(tags('<input v-model="a" />'), ['input'])
  assert.deepEqual(tags('<input type="text" />'), ['input'])
  assert.deepEqual(tags('<input :type="kind" />'), ['input'])
  assert.deepEqual(tags('<textarea /><select />'), ['textarea', 'select'])
  assert.deepEqual(tags('<input type="checkbox" />'), [])
  assert.deepEqual(tags('<input type="radio" @change="(e) => pick(e)" />'), [])
  assert.deepEqual(tags('<input type="range" /><input type="file" />'), [])
  assert.deepEqual(tags('<UiInput /><UiTextarea /><UiSelect />'), [])
  assert.deepEqual(
    findOldFields('<script setup>// <textarea> in a comment\n</script>'),
    [],
  )
})

test('every field in src/ is a haex-ui field, apart from the listed files', () => {
  const files = execFileSync('git', ['ls-files', '-z', '--', 'src/**/*.vue'], {
    encoding: 'utf8',
  })
    .split('\0')
    .filter(Boolean)
  assert.ok(files.length > 0, 'no tracked .vue files found')

  const found: string[] = []
  for (const file of files) {
    for (const hit of findOldFields(readFileSync(file, 'utf8'), file)) {
      if (!(file in ALLOWED)) found.push(`${file}:${hit.line} <${hit.tag}>`)
    }
  }
  assert.deepEqual(
    found,
    [],
    'use UiInput, UiInputPassword, UiTextarea or UiSelect (or list the file in ALLOWED with a reason)',
  )
})

test('every allowed file still holds an older field (the list stays honest)', () => {
  for (const file of Object.keys(ALLOWED)) {
    const hits = findOldFields(readFileSync(file, 'utf8'), file)
    assert.ok(hits.length > 0, `${file} no longer needs its entry in ALLOWED`)
  }
})
