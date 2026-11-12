// Part of `pnpm check:settings` (spec 055, T056): the address check of the model-server form and
// the user's statements about a model (src/lib/models/servers.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  acceptsImagesOf,
  addressProblem,
  groupByKind,
  imagesChoice,
  imagesReported,
  levelsText,
  parseLevels,
  showsLevels,
  suggestsLevels,
} from '../src/lib/models/servers.ts'

test('a server address is http or https with a host and nothing else', () => {
  for (const good of [
    'http://192.168.1.20:11434/v1',
    'https://api.example.com/v1',
    '  http://localhost:8080/v1  ',
    'http://[fd00::5]:8080/v1',
  ])
    assert.equal(addressProblem(good), null, good)
  assert.equal(addressProblem(''), 'empty')
  assert.equal(addressProblem('   '), 'empty')
  assert.equal(addressProblem('192.168.1.20:11434'), 'scheme')
  assert.equal(addressProblem('server/v1'), 'scheme')
  assert.equal(addressProblem('localhost:8080/v1'), 'scheme')
  assert.equal(addressProblem('http://'), 'invalid')
  assert.equal(addressProblem('ftp://example.com'), 'scheme')
  assert.equal(addressProblem('file:///etc/passwd'), 'scheme')
  assert.equal(
    addressProblem('https://user:sk-123@example.com/v1'),
    'credentials',
  )
  assert.equal(addressProblem('https://example.com/v1?key=sk-123'), 'query')
  assert.equal(addressProblem('https://example.com/v1#x'), 'query')
})

test('images are stated, denied or left open', () => {
  assert.equal(imagesChoice(null), 'unstated')
  assert.equal(
    imagesChoice({ acceptsImages: null, effortLevels: null }),
    'unstated',
  )
  assert.equal(imagesChoice({ acceptsImages: true, effortLevels: null }), 'yes')
  assert.equal(imagesChoice({ acceptsImages: false, effortLevels: null }), 'no')
  assert.equal(acceptsImagesOf('unstated'), null)
  assert.equal(acceptsImagesOf('yes'), true)
  assert.equal(acceptsImagesOf('no'), false)
})

test('effort levels are words separated by commas or blanks, an empty field states nothing', () => {
  assert.deepEqual(parseLevels('low, medium,high'), ['low', 'medium', 'high'])
  assert.deepEqual(parseLevels('  low   high '), ['low', 'high'])
  assert.equal(parseLevels(''), null)
  assert.equal(parseLevels(' , '), null)
  assert.equal(levelsText(['low', 'high']), 'low, high')
  assert.equal(levelsText(null), '')
})

const none = { acceptsImages: null, effortLevels: null }

test('models are grouped by kind; a model the server does not describe is a chat model', () => {
  const models = [
    { name: 'a', capabilities: { kind: 'image' as const } },
    { name: 'b', capabilities: null },
    { name: 'c', capabilities: { kind: 'text' as const } },
    { name: 'd', capabilities: { kind: 'other' as const } },
  ]
  assert.deepEqual(
    groupByKind(models).map((g) => [g.kind, g.models.map((m) => m.name)]),
    [
      ['text', ['b', 'c']],
      ['image', ['a']],
      ['other', ['d']],
    ],
  )
  assert.deepEqual(groupByKind([]), [])
})

test('the images choice is only needed where the server does not say', () => {
  assert.equal(imagesReported(null), null)
  assert.equal(imagesReported({ acceptedAttachmentKinds: null }), null)
  assert.equal(
    imagesReported({ acceptedAttachmentKinds: ['text', 'image'] }),
    true,
  )
  assert.equal(imagesReported({ acceptedAttachmentKinds: ['text'] }), false)
  // A statement of the user keeps the choice open to change or withdraw.
  assert.equal(
    imagesReported({
      acceptedAttachmentKinds: ['text'],
      user: { acceptsImages: true, effortLevels: null },
    }),
    null,
  )
})

test('the effort field shows for reasoning models and for servers that describe nothing', () => {
  assert.equal(showsLevels(null), true)
  assert.equal(showsLevels({ kind: 'text', reasoning: null }), false)
  assert.equal(
    showsLevels({ kind: 'text', reasoning: { kind: 'unavailable' } }),
    false,
  )
  assert.equal(
    showsLevels({ kind: 'text', reasoning: { kind: 'model_managed' } }),
    true,
  )
  assert.equal(
    showsLevels({
      kind: 'text',
      reasoning: { kind: 'unavailable' },
      user: { acceptsImages: null, effortLevels: ['low'] },
    }),
    true,
  )
})

test('the level suggestion is for reasoning models without stated levels', () => {
  assert.equal(
    suggestsLevels({ reasoning: { kind: 'model_managed' }, user: none }),
    true,
  )
  assert.equal(
    suggestsLevels({
      reasoning: { kind: 'presets' },
      user: { acceptsImages: null, effortLevels: ['low'] },
    }),
    false,
  )
  assert.equal(suggestsLevels({ reasoning: null }), false)
})
