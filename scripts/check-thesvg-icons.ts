// `pnpm check:thesvg-icons` (spec 103, T004, T005, T025): the pure rules of the theSVG icon set
// build (`scripts/icon-sets/thesvg.ts`): the colour range, the mapping checks, the colouring of a
// source SVG and the build itself on a fake clone.
import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { test } from 'node:test'

import { parseKinds } from './icon-sets/kinds.ts'
import {
  checkMapping,
  colorize,
  colorProblem,
  luminance,
  prepareThesvg,
  ROLE_SLUGS,
  usedSlugs,
} from './icon-sets/thesvg.ts'

const PALETTE = [
  '#42a5f5',
  '#90a4ae',
  '#26a69a',
  '#ff9800',
  '#ec407a',
  '#e53935',
  '#64b5f6',
  '#7e57c2',
  '#afb42b',
  '#f9a825',
  '#0288d1',
  '#78909c',
  '#66bb6a',
  '#8d6e63',
  '#ffb300',
  '#29b6f6',
  '#ff7043',
  '#43a047',
]

test('luminance is the WCAG relative luminance', () => {
  assert.ok(Math.abs(luminance('#42a5f5') - 0.347) < 0.002)
  assert.equal(luminance('#000000'), 0)
  assert.ok(Math.abs(luminance('#ffffff') - 1) < 1e-9)
})

test('every colour of the palette is readable on both schemes (FR-005)', () => {
  for (const color of PALETTE) assert.equal(colorProblem(color), null, color)
})

test('too dark, too light and malformed colours are refused (FR-005)', () => {
  for (const color of ['#000000', '#101010', '#ffffff', '#fafafa']) {
    assert.match(colorProblem(color) ?? '', /luminance/, color)
  }
  for (const color of ['red', '#fff', '#12345', '#gggggg', '42a5f5', '']) {
    assert.match(colorProblem(color) ?? '', /#rrggbb/, color)
  }
})

test('colorize replaces every currentColor and nothing else', () => {
  const svg =
    '<svg><path fill="currentColor" d="M0 0"/><g stroke="currentColor"/><mask id="m"><rect fill="#fff"/></mask></svg>'
  assert.equal(
    colorize(svg, '#42a5f5'),
    '<svg><path fill="#42a5f5" d="M0 0"/><g stroke="#42a5f5"/><mask id="m"><rect fill="#fff"/></mask></svg>',
  )
  assert.throws(
    () => colorize('<svg><path d="M0 0"/></svg>', '#42a5f5'),
    /currentColor/,
  )
})

const source = new Set(['file', 'folder', 'image', 'file-code'])
const colors = {
  file: '#90a4ae',
  folder: '#42a5f5',
  image: '#26a69a',
  'file-code': '#7e57c2',
}
const good = {
  kinds: parseKinds({
    kinds: {
      image: { extensions: ['png'] },
      code: { extensions: ['rs'] },
      readme: { names: ['readme'] },
    },
  }),
  table: { image: 'image', code: 'file-code', readme: 'file' },
  roles: { folder: 'folder', file: 'file', image: 'image' },
  colors,
  slugs: source,
}

test('a valid mapping has no problems', () => {
  assert.deepEqual(checkMapping(good), [])
})

test('a slug that the source does not have is named (data-model rule 1)', () => {
  const problems = checkMapping({
    ...good,
    table: { code: 'file-codee' },
  })
  assert.ok(
    problems.some((p) => p.includes('file-codee')),
    problems.join('\n'),
  )
  const role = checkMapping({
    ...good,
    roles: { ...good.roles, pdf: 'pdf-fil' },
  })
  assert.ok(role.some((p) => p.includes('pdf-fil')))
})

test('a used slug without a colour is named (data-model rule 2)', () => {
  const problems = checkMapping({
    ...good,
    table: { code: 'file-code' },
    colors: { file: '#90a4ae', folder: '#42a5f5', image: '#26a69a' },
  })
  assert.ok(
    problems.some((p) => p.includes('file-code') && p.includes('colour')),
  )
})

test('a colour that is out of range is named with its slug (data-model rule 2)', () => {
  const problems = checkMapping({
    ...good,
    colors: { ...colors, image: '#ffffff' },
  })
  assert.ok(problems.some((p) => p.includes('image') && p.includes('#ffffff')))
})

test('a kind of the table that kinds.json does not have is named', () => {
  const problems = checkMapping({
    ...good,
    table: { ...good.table, 'folder-downloadz': 'file' },
  })
  assert.ok(problems.some((p) => p.includes('folder-downloadz')))
})

// A fake clone of thesvg (a git repository with the files the build reads) and a fake
// `icon-sets` directory with the checked-in inputs, both outside the repository.
function fixture(): {
  clone: string
  sets: string
  commit: string
  cleanup: () => void
} {
  const root = mkdtempSync(path.join(tmpdir(), 'thesvg-check-'))
  const clone = path.join(root, 'clone')
  const sets = path.join(root, 'sets')
  mkdirSync(path.join(clone, 'src', 'data', 'ui', 'looks'), { recursive: true })
  mkdirSync(sets, { recursive: true })
  const slugs = [...usedSlugs({ table: GOOD_TABLE, roles: ROLE_SLUGS })]
  const svgs = Object.fromEntries(
    slugs.map((slug) => [
      slug,
      `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="currentColor" d="M0 0h24v24H0z"/></svg>`,
    ]),
  )
  writeFileSync(
    path.join(clone, 'src', 'data', 'ui', 'looks', 'fill.json'),
    JSON.stringify(svgs),
  )
  writeFileSync(
    path.join(clone, 'LICENSE'),
    'MIT License\n\nCopyright (c) 2025 thesvg.org\n',
  )
  const git = (...args: string[]) =>
    execFileSync(
      'git',
      [
        '-C',
        clone,
        '-c',
        'user.name=check',
        '-c',
        'user.email=check@example.invalid',
        ...args,
      ],
      {
        encoding: 'utf8',
      },
    ).trim()
  git('init', '-q')
  git('add', '-A')
  git('commit', '-q', '-m', 'fixture')
  const commit = git('rev-parse', 'HEAD')
  writeFileSync(
    path.join(sets, 'thesvg.source.json'),
    JSON.stringify({
      repository: 'https://github.com/glincker/thesvg',
      commit,
    }),
  )
  writeFileSync(path.join(sets, 'kinds.json'), JSON.stringify(GOOD_KINDS))
  writeFileSync(
    path.join(sets, 'thesvg.kinds.json'),
    JSON.stringify({ kinds: GOOD_TABLE }),
  )
  writeFileSync(
    path.join(sets, 'thesvg.colors.json'),
    JSON.stringify(Object.fromEntries(slugs.map((slug) => [slug, '#42a5f5']))),
  )
  return {
    clone,
    sets,
    commit,
    cleanup: () => rmSync(root, { recursive: true, force: true }),
  }
}

const GOOD_KINDS = {
  kinds: {
    code: { extensions: ['rs'] },
    readme: { names: ['readme'] },
    'folder-downloads': { names: ['downloads', 'téléchargements'] },
  },
}
const GOOD_TABLE = {
  code: 'file-code',
  readme: 'file-text',
  'folder-downloads': 'file-code',
}

test('the build writes the pack source from a valid clone (US3 scenario 1)', () => {
  const f = fixture()
  try {
    prepareThesvg(f.clone, f.sets)
    const manifest = JSON.parse(
      readFileSync(path.join(f.sets, 'thesvg', 'icon-theme.json'), 'utf8'),
    )
    assert.equal(manifest.fileExtensions.rs, 'file-code')
    assert.equal(manifest.folderNames.downloads, 'file-code')
    assert.equal(manifest.folderNames['téléchargements'], 'file-code')
    assert.equal(manifest['x-haex'].origin.license, 'MIT')
    assert.equal(manifest['x-haex'].roles.audio, 'audio-lines')
    assert.equal(
      manifest['x-haex'].roles.encryptedFolder,
      'haex-encrypted-folder',
    )
    const icon = readFileSync(
      path.join(f.sets, 'thesvg', 'icons', 'file-code.svg'),
      'utf8',
    )
    assert.ok(icon.includes('#42a5f5') && !icon.includes('currentColor'))
    const first = readFileSync(
      path.join(f.sets, 'thesvg', 'icon-theme.json'),
      'utf8',
    )
    prepareThesvg(f.clone, f.sets)
    assert.equal(
      readFileSync(path.join(f.sets, 'thesvg', 'icon-theme.json'), 'utf8'),
      first,
    )
  } finally {
    f.cleanup()
  }
})

test('the build reads the pinned SVGs and license when the clone has local edits', () => {
  const f = fixture()
  try {
    const fillPath = path.join(
      f.clone,
      'src',
      'data',
      'ui',
      'looks',
      'fill.json',
    )
    const licensePath = path.join(f.clone, 'LICENSE')
    const pinnedLicense = readFileSync(licensePath, 'utf8')

    prepareThesvg(f.clone, f.sets)
    const expectedIcon = readFileSync(
      path.join(f.sets, 'thesvg', 'icons', 'file-code.svg'),
      'utf8',
    )

    const editedSvgs = JSON.parse(readFileSync(fillPath, 'utf8'))
    editedSvgs['file-code'] = editedSvgs['file-code'].replaceAll(
      'currentColor',
      '#ffffff',
    )
    writeFileSync(fillPath, JSON.stringify(editedSvgs))
    writeFileSync(licensePath, 'MIT License\nlocally edited license\n')

    prepareThesvg(f.clone, f.sets)

    assert.equal(
      readFileSync(
        path.join(f.sets, 'thesvg', 'icons', 'file-code.svg'),
        'utf8',
      ),
      expectedIcon,
    )
    assert.equal(
      readFileSync(path.join(f.sets, 'thesvg', 'LICENSE'), 'utf8'),
      pinnedLicense,
    )
  } finally {
    f.cleanup()
  }
})

test('the build refuses a clone at another commit and names both (US3 scenario 2)', () => {
  const f = fixture()
  try {
    writeFileSync(
      path.join(f.sets, 'thesvg.source.json'),
      JSON.stringify({ commit: '0'.repeat(40) }),
    )
    assert.throws(
      () => prepareThesvg(f.clone, f.sets),
      (error: Error) =>
        error.message.includes(f.commit) &&
        error.message.includes('0'.repeat(40)),
    )
  } finally {
    f.cleanup()
  }
})

test('the build refuses a missing icon and a bad colour, naming them (US3)', () => {
  const f = fixture()
  try {
    writeFileSync(
      path.join(f.sets, 'thesvg.kinds.json'),
      JSON.stringify({ kinds: { code: 'no-such-icon' } }),
    )
    assert.throws(() => prepareThesvg(f.clone, f.sets), /no-such-icon/)
    writeFileSync(
      path.join(f.sets, 'thesvg.kinds.json'),
      JSON.stringify({ kinds: GOOD_TABLE }),
    )
    const colors = JSON.parse(
      readFileSync(path.join(f.sets, 'thesvg.colors.json'), 'utf8'),
    )
    colors['file-code'] = '#ffffff'
    writeFileSync(
      path.join(f.sets, 'thesvg.colors.json'),
      JSON.stringify(colors),
    )
    assert.throws(() => prepareThesvg(f.clone, f.sets), /file-code.*#ffffff/s)
  } finally {
    f.cleanup()
  }
})
