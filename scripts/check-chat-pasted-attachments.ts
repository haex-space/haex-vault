// Part of `pnpm check:chat-state` (spec 011 FR-019): what a paste into the composer turns into.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  filePathsFromUriList,
  hasPastedFiles,
  pastedFileName,
  readPaste,
} from '../src/lib/chat/pastedAttachments.ts'

function file(name: string, type: string): File {
  return new File(['x'], name, { type })
}

function data(files: File[], formats: Record<string, string> = {}) {
  return { files, getData: (format: string) => formats[format] ?? '' }
}

test('a paste of plain text carries no file', () => {
  const paste = readPaste(data([], { 'text/plain': 'hello' }))
  assert.equal(hasPastedFiles(paste), false)
})

test('a missing clipboard carries no file', () => {
  assert.equal(hasPastedFiles(readPaste(null)), false)
})

test('a pasted image is taken by its bytes', () => {
  const image = file('image.png', 'image/png')
  const paste = readPaste(data([image]))
  assert.deepEqual(paste.files, [image])
  assert.deepEqual(paste.paths, [])
  assert.equal(hasPastedFiles(paste), true)
})

test('files copied in a file manager are taken by their path', () => {
  const paste = readPaste(
    data([file('a.pdf', 'application/pdf')], {
      'text/uri-list': 'file:///home/me/a.pdf\r\nfile:///home/me/b%20c.txt\r\n',
    }),
  )
  assert.deepEqual(paste.paths, [
    'file:///home/me/a.pdf',
    'file:///home/me/b%20c.txt',
  ])
  assert.deepEqual(paste.files, [])
})

test('a copied web link is no file', () => {
  assert.deepEqual(filePathsFromUriList('https://example.com/a.png'), [])
  const paste = readPaste(data([], { 'text/uri-list': 'https://example.com' }))
  assert.equal(hasPastedFiles(paste), false)
})

test('a list that mixes files and web addresses is no file paste', () => {
  assert.deepEqual(
    filePathsFromUriList('file:///a.png\r\nhttps://example.com'),
    [],
  )
})

test('comments and blank lines of a uri list are skipped', () => {
  assert.deepEqual(filePathsFromUriList('# copied\r\n\r\nfile:///a.png\r\n'), [
    'file:///a.png',
  ])
})

test('a name with an extension is kept', () => {
  assert.equal(pastedFileName(file('image.png', 'image/png')), 'image.png')
  assert.equal(
    pastedFileName(file('report.v2.pdf', 'application/pdf')),
    'report.v2.pdf',
  )
})

test('a name without an extension gets the one of its type', () => {
  assert.equal(
    pastedFileName(file('Screenshot', 'image/jpeg')),
    'Screenshot.jpg',
  )
  assert.equal(pastedFileName(file('', 'image/png')), 'pasted.png')
})

test('a name without an extension and of an unknown type stays as it is', () => {
  assert.equal(pastedFileName(file('notes', 'application/zip')), 'notes')
})
