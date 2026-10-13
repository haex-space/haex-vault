// Spec 038, research R6, T041: the storage dialog over an extension's tab never has a field for
// credentials. An extension can draw a look-alike inside its frame, so holzi asks for credentials
// only in its window over the whole app (`StorageCredentialsModal.vue`). This walks the template of
// `StorageDialog.vue`: radio buttons and checkboxes are fine, every other input is not.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { parse } from 'vue/compiler-sfc'

type Node = {
  type: number
  tag?: string
  props?: {
    type: number
    name: string
    value?: { content: string }
    arg?: { content?: string }
  }[]
  children?: Node[]
}

const ELEMENT = 1
const ATTRIBUTE = 6
const DIRECTIVE = 7
const TEXT_FIELDS = new Set([
  'textarea',
  'uiinput',
  'uiinputpassword',
  'uitextarea',
  'shadcninput',
  'shadcntextarea',
  'ui-input',
  'ui-input-password',
])

function fields(node: Node, found: string[] = []): string[] {
  if (node.type === ELEMENT && node.tag) {
    const tag = node.tag.toLowerCase()
    const attribute = (name: string) =>
      node.props?.find((p) => p.type === ATTRIBUTE && p.name === name)?.value
        ?.content
    if (TEXT_FIELDS.has(tag)) found.push(tag)
    if (tag === 'input') {
      const type = attribute('type') ?? 'text'
      if (type !== 'radio' && type !== 'checkbox') found.push(`input[${type}]`)
    }
    // Present at all, also bare (`<div contenteditable>`) or bound (`:contenteditable`).
    const editable = node.props?.some(
      (p) =>
        (p.type === ATTRIBUTE && p.name === 'contenteditable') ||
        (p.type === DIRECTIVE &&
          p.name === 'bind' &&
          p.arg?.content === 'contenteditable'),
    )
    if (editable) found.push(`${tag}[contenteditable]`)
  }
  for (const child of node.children ?? []) fields(child, found)
  return found
}

function hasElement(node: Node, tagName: string): boolean {
  if (node.type === ELEMENT && node.tag?.toLowerCase() === tagName)
    return true
  return (node.children ?? []).some((child) => hasElement(child, tagName))
}

function parsed(source: string, filename: string): Node {
  const { descriptor, errors } = parse(source, { filename })
  assert.deepEqual(errors, [])
  assert.ok(descriptor.template?.ast, filename)
  return descriptor.template.ast as unknown as Node
}

function template(path: string): Node {
  return parsed(readFileSync(new URL(path, import.meta.url), 'utf8'), path)
}

test('the check finds every kind of field it promises to find', () => {
  const found = fields(
    parsed(
      `<template><div contenteditable /><p :contenteditable="x" /><input /><input type="radio" /><UiInputPassword /></template>`,
      'fixture.vue',
    ),
  )
  assert.deepEqual(found, [
    'div[contenteditable]',
    'p[contenteditable]',
    'input[text]',
    'uiinputpassword',
  ])
})

test('the storage dialog over a tab has no field for credentials', () => {
  assert.deepEqual(
    fields(template('../src/components/extensions/StorageDialog.vue')),
    [],
  )
})

test('the credentials are typed in the window over the whole app', () => {
  const modal = template('../src/components/StorageCredentialsModal.vue')
  assert.ok(hasElement(modal, 'credentialsfields'))

  const credentials = fields(
    template('../src/components/settings/storage/CredentialsFields.vue'),
  )
  assert.ok(credentials.includes('uiinputpassword'), JSON.stringify(credentials))
})
