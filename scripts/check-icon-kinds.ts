// Checks of the shared mapping of file kinds (`scripts/icon-sets/kinds.ts`, spec 103 research R7):
// the rules of kinds.json, the fallback along the parent chain, and that the checked-in mapping and
// the tables of the sets agree. Run through `pnpm check:file-icons`.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import path from 'node:path'
import { test } from 'node:test'

import { ROOT } from './icon-sets/common.ts'
import {
  addKindFolderNames,
  imageOf,
  kindsProblems,
  parseKinds,
  readKinds,
  resolveKinds,
  tableProblems,
} from './icon-sets/kinds.ts'

const SETS = path.join(ROOT, 'src-tauri', 'icon-sets')

const kinds = parseKinds({
  kinds: {
    code: { extensions: ['c'], names: ['makefile'] },
    rust: { parent: 'code', extensions: ['rs'] },
    pdf: { extensions: ['pdf'] },
    'folder-downloads': { names: ['downloads', 'bilder-x'] },
  },
})

test('a valid mapping has no problems', () => {
  assert.deepEqual(kindsProblems(kinds), [])
})

test('an extension or name in two kinds, upper case and a dotted extension are named', () => {
  const problems = kindsProblems(
    parseKinds({
      kinds: {
        a: { extensions: ['rs', 'PDF', '.md'] },
        b: { extensions: ['rs'], names: ['Makefile'] },
        'folder-x': { names: ['Bilder'] },
      },
    }),
  )
  const text = problems.join('\n')
  assert.match(text, /extension rs is in a and in b/)
  assert.match(text, /PDF must be in lower case/)
  assert.match(text, /\.md is written without its dot/)
  assert.match(text, /Makefile must be in lower case/)
  assert.match(text, /Bilder must be in lower case/)
})

test('a file name and a folder name may be the same word', () => {
  const problems = kindsProblems(
    parseKinds({
      kinds: {
        readme: { names: ['docs'] },
        'folder-docs': { names: ['docs'] },
      },
    }),
  )
  assert.deepEqual(problems, [])
})

test('a missing parent, a loop and a folder kind with a parent are named', () => {
  const text = kindsProblems(
    parseKinds({
      kinds: {
        a: { parent: 'nope' },
        b: { parent: 'c' },
        c: { parent: 'b' },
        'folder-x': { parent: 'a' },
      },
    }),
  ).join('\n')
  assert.match(text, /a: the parent nope does not exist/)
  assert.match(text, /loops/)
  assert.match(text, /folder-x: a folder kind has no parent/)
})

test('a kind takes the image of the nearest parent that has one', () => {
  assert.equal(imageOf(kinds, { rust: 'r', code: 'c' }, 'rust'), 'r')
  assert.equal(imageOf(kinds, { code: 'c' }, 'rust'), 'c')
  assert.equal(imageOf(kinds, { pdf: 'p' }, 'rust'), undefined)
  // a folder kind never falls back to anything
  assert.equal(imageOf(kinds, { code: 'c' }, 'folder-downloads'), undefined)
})

test('resolving gives every extension, name and folder name the image of its kind', () => {
  const resolved = resolveKinds(kinds, {
    code: 'c',
    'folder-downloads': 'd',
  })
  assert.deepEqual({ ...resolved.extensions }, { c: 'c', rs: 'c' })
  assert.deepEqual({ ...resolved.names }, { makefile: 'c' })
  assert.deepEqual({ ...resolved.folders }, { downloads: 'd', 'bilder-x': 'd' })
})

test('names matching Object.prototype keys are stored as own mapping keys', () => {
  const resolved = resolveKinds(
    {
      code: { extensions: ['__proto__'], names: ['__proto__'] },
      'folder-special': { extensions: [], names: ['__proto__'] },
    },
    { code: 'code-image', 'folder-special': 'folder-image' },
  )

  assert.equal(Object.getPrototypeOf(resolved.extensions), null)
  assert.equal(Object.getPrototypeOf(resolved.names), null)
  assert.equal(Object.getPrototypeOf(resolved.folders), null)
  assert.equal(Object.hasOwn(resolved.extensions, '__proto__'), true)
  assert.equal(resolved.extensions['__proto__'], 'code-image')
  assert.equal(Object.hasOwn(resolved.names, '__proto__'), true)
  assert.equal(resolved.names['__proto__'], 'code-image')
  assert.equal(Object.hasOwn(resolved.folders, '__proto__'), true)
  assert.equal(resolved.folders['__proto__'], 'folder-image')
})

test('a kind that the table names but kinds.json does not have is named', () => {
  assert.deepEqual(tableProblems(kinds, { rust: 'r' }, 'x.json'), [])
  assert.deepEqual(tableProblems(kinds, { rustt: 'r' }, 'x.json'), [
    'x.json: rustt is not a kind of kinds.json',
  ])
})

test('a set that brings its own mapping gets the other folder names of a kind', () => {
  const names = { downloads: 'folder-download', other: 'x' }
  assert.equal(addKindFolderNames(kinds, names, 'material'), 1)
  assert.equal(names['bilder-x'], 'folder-download')
  // a name the set maps itself stays
  const own = { downloads: 'a', 'bilder-x': 'b' }
  assert.equal(addKindFolderNames(kinds, own, 'material'), 0)
  assert.equal(own['bilder-x'], 'b')
  assert.throws(
    () => addKindFolderNames(kinds, { other: 'x' }, 'material'),
    /material has no folder image for folder-downloads/,
  )
})

// The checked-in files.

const real = readKinds(path.join(SETS, 'kinds.json'))

function table(name: string): Record<string, string> {
  const raw = JSON.parse(readFileSync(path.join(SETS, name), 'utf8'))
  return raw.kinds
}

test('the checked-in kinds.json has no problems and covers the common extensions', () => {
  assert.deepEqual(kindsProblems(real), [])
  const extensions = Object.values(real).flatMap((kind) => kind.extensions)
  assert.ok(extensions.length >= 150, `${extensions.length} extensions`)
  for (const extension of ['rs', 'ts', 'py', 'pdf', 'png', 'mp3', 'zip']) {
    assert.ok(extensions.includes(extension), extension)
  }
})

test('every table of a set names only kinds of kinds.json', () => {
  for (const name of ['thesvg.kinds.json', 'phosphor.kinds.json']) {
    assert.deepEqual(tableProblems(real, table(name), name), [], name)
  }
})

test('two sets give one extension the image of its own kind or of its parent', () => {
  const thesvg = resolveKinds(real, table('thesvg.kinds.json'))
  const phosphor = resolveKinds(real, table('phosphor.kinds.json'))
  assert.equal(thesvg.extensions.rs, 'file-code') // theSVG has no Rust image: the parent code
  assert.equal(phosphor.extensions.rs, 'file-rs') // Phosphor has one
  assert.equal(thesvg.extensions.pdf, 'pdf-file')
  assert.equal(phosphor.extensions.pdf, 'file-pdf')
  assert.equal(thesvg.names.readme, 'readme-file')
  assert.equal(phosphor.names.readme, undefined) // no image for a readme in Phosphor: generic
  assert.equal(thesvg.folders['téléchargements'], 'download')
  assert.equal(thesvg.folders.downloads, 'download')
})

test('the Material pack knows every folder name of kinds.json', () => {
  const manifest = JSON.parse(
    readFileSync(path.join(SETS, 'material', 'icon-theme.json'), 'utf8'),
  )
  const missing = Object.entries(real)
    .filter(([id]) => id.startsWith('folder-'))
    .flatMap(([, kind]) => kind.names)
    .filter((name) => !(name in manifest.folderNames))
  assert.deepEqual(missing, [])
})
