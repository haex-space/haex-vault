// `pnpm check:file-icons` (spec 053, T031): the pure helpers of the file icons.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  batches,
  createMemory,
  imageKey,
  MAX_QUERIES,
  queryKey,
  selectionValue,
} from '../src/lib/files/fileIcons.ts'

const lucide = { kind: 'builtin', name: 'lucide' } as const
const phosphor = { kind: 'builtin', name: 'phosphor' } as const
const custom = {
  kind: 'custom',
  id: '0b9a2c52-6a53-4f4e-9a1b-3c1a1d0a77aa',
} as const

const file = (name: string) => ({
  name,
  isDir: false,
  noAccess: false,
  encrypted: null,
})

test('the value of a set is the one the preference takes', () => {
  assert.equal(selectionValue(lucide), 'builtin:lucide')
  assert.equal(selectionValue(phosphor), 'builtin:phosphor')
  assert.equal(selectionValue(custom), `custom:${custom.id}`)
})

test('the key of an answer is stable and tells every input apart', () => {
  const key = queryKey(lucide, 'dark', file('a.rs'))
  assert.equal(key, queryKey(lucide, 'dark', { ...file('a.rs') }))
  const others = [
    queryKey(phosphor, 'dark', file('a.rs')),
    queryKey(lucide, 'light', file('a.rs')),
    queryKey(lucide, 'dark', file('b.rs')),
    queryKey(lucide, 'dark', { ...file('a.rs'), isDir: true }),
    queryKey(lucide, 'dark', { ...file('a.rs'), noAccess: true }),
    queryKey(lucide, 'dark', {
      ...file('a.rs'),
      encrypted: { role: 'inside', state: 'damaged', foreign: 0 },
    }),
  ]
  assert.equal(new Set([key, ...others]).size, others.length + 1)
})

test('the number of foreign files does not change the key', () => {
  const folder = (foreign: number) =>
    queryKey(lucide, 'dark', {
      name: 'x',
      isDir: true,
      noAccess: false,
      encrypted: { role: 'folder', state: 'readable', foreign },
    })
  assert.equal(folder(0), folder(7))
})

test('an image is kept by set and id', () => {
  assert.notEqual(imageKey(lucide, 'a.svg'), imageKey(phosphor, 'a.svg'))
  assert.notEqual(imageKey(phosphor, 'a.svg'), imageKey(phosphor, 'b.svg'))
})

test('the questions of one frame go out in calls of at most 500, each key once', () => {
  const asked = Array.from({ length: 1200 }, (_, index) => ({
    key: `k${index % 1000}`,
  }))
  const groups = batches(asked)
  assert.deepEqual(
    groups.map((group) => group.length),
    [500, 500],
  )
  assert.equal(groups.flat().length, 1000)
  assert.equal(new Set(groups.flat().map((item) => item.key)).size, 1000)
  assert.ok(groups.every((group) => group.length <= MAX_QUERIES))
  assert.deepEqual(batches([]), [])
})

test('the memory is bounded: the oldest leave first and a leaving value is handed back', () => {
  const left: string[] = []
  const memory = createMemory<string>(3, (value) => left.push(value))
  for (const key of ['a', 'b', 'c']) memory.set(key, `blob:${key}`)
  memory.get('a')
  memory.set('d', 'blob:d')
  assert.deepEqual(left, ['blob:b'])
  assert.equal(memory.get('b'), undefined)
  assert.equal(memory.get('a'), 'blob:a')
  assert.equal(memory.size(), 3)
})

test('replacing a value hands the old one back; clearing hands back all', () => {
  const left: string[] = []
  const memory = createMemory<string>(5, (value) => left.push(value))
  memory.set('a', 'blob:1')
  memory.set('a', 'blob:2')
  assert.deepEqual(left, ['blob:1'])
  memory.set('b', 'blob:3')
  memory.clear()
  assert.deepEqual(left.sort(), ['blob:1', 'blob:2', 'blob:3'])
  assert.equal(memory.size(), 0)
})
