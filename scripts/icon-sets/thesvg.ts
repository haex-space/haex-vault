// Spec 103: writes the source directory of the bundled file icon set "theSVG",
// `src-tauri/icon-sets/thesvg/`, from the fill look of the UI icons of thesvg.org (MIT), coloured by
// kind. Like `prepare.ts` it never writes a ZIP (Rust packs the directory) and takes the clone path
// from the command line only.
//
// Inputs, all checked in next to the result (data-model.md):
//   thesvg.source.json  repository and the pinned commit; the clone must be at that commit
//   kinds.json          the mapping of extensions, file names and folder names to kinds, shared by
//                       every set (kinds.ts)
//   thesvg.kinds.json   { kinds: { kind: slug } }, the image of this set for a kind
//   thesvg.colors.json  { slug: '#rrggbb' } for every slug the table and the roles use
import { execFileSync } from 'node:child_process'
import { writeFileSync } from 'node:fs'
import path from 'node:path'

import {
  addStateIcons,
  fail,
  freshDir,
  isRecord,
  readJson,
  readStringMap,
  SETS_DIR,
  writeJson,
} from './common.ts'
import type { Origin, Roles, StringMap } from './common.ts'
import { readKinds, resolveKinds, tableProblems } from './kinds.ts'
import type { Kinds } from './kinds.ts'

/** The role images of the set, by source slug (the states come from `haex-states/`). */
export const ROLE_SLUGS: Roles = {
  folder: 'folder',
  file: 'file',
  image: 'image',
  video: 'video',
  audio: 'audio-lines',
  pdf: 'pdf-file',
  text: 'file-text',
}

const ORIGIN: Origin = {
  author: 'thesvg.org',
  license: 'MIT',
  source: 'https://github.com/glincker/thesvg',
}

// FR-005: readable on the dark and on the light scheme, so no image needs a second version.
const MIN_LUMINANCE = 0.1
const MAX_LUMINANCE = 0.65

/** WCAG relative luminance of `#rrggbb`. */
export function luminance(color: string): number {
  const channel = (offset: number): number => {
    const value = parseInt(color.slice(offset, offset + 2), 16) / 255
    return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4
  }
  return 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5)
}

/** Why a colour cannot be used, or `null` when it can. */
export function colorProblem(color: string): string | null {
  if (!/^#[0-9a-f]{6}$/.test(color))
    return `${color} is not a colour of the form #rrggbb`
  const value = luminance(color)
  if (value < MIN_LUMINANCE || value > MAX_LUMINANCE) {
    return `${color} has a luminance of ${value.toFixed(3)}, outside ${MIN_LUMINANCE} to ${MAX_LUMINANCE}`
  }
  return null
}

/** The SVG with every `currentColor` replaced by `color`; nothing else changes. */
export function colorize(svg: string, color: string): string {
  if (!svg.includes('currentColor'))
    fail('an icon has no currentColor to colour')
  return svg.replaceAll('currentColor', color)
}

export type MappingInput = {
  kinds: Kinds
  /** The image (a slug of the source) of this set for a kind. */
  table: StringMap
  roles: Roles
  colors: StringMap
  slugs: ReadonlySet<string>
}

/** Every slug the table and the roles use. */
export function usedSlugs(
  input: Pick<MappingInput, 'table' | 'roles'>,
): Set<string> {
  return new Set([
    ...Object.values(input.table),
    ...Object.values(input.roles).filter(
      (slug): slug is string => slug !== undefined,
    ),
  ])
}

/** The problems of the build inputs (data-model.md rules 1 and 2); empty when they are fine. */
export function checkMapping(input: MappingInput): string[] {
  const problems = tableProblems(input.kinds, input.table, 'thesvg.kinds.json')
  const used = usedSlugs(input)
  for (const slug of [...used].sort()) {
    if (!input.slugs.has(slug))
      problems.push(`${slug}: the source has no such icon`)
    const color = input.colors[slug]
    if (color === undefined) {
      problems.push(`${slug}: no colour in thesvg.colors.json`)
      continue
    }
    const problem = colorProblem(color)
    if (problem !== null) problems.push(`${slug}: ${problem}`)
  }
  for (const slug of Object.keys(input.colors).sort()) {
    if (!used.has(slug))
      problems.push(`${slug}: has a colour but nothing uses it`)
  }
  return problems
}

function readTable(file: string): StringMap {
  const raw = readJson(file)
  if (!isRecord(raw)) fail(`${path.basename(file)} is not an object`)
  return readStringMap(raw.kinds, `${path.basename(file)} kinds`)
}

function headOf(clone: string): string {
  return execFileSync('git', ['-C', clone, 'rev-parse', 'HEAD'], {
    encoding: 'utf8',
  }).trim()
}

function readPinnedFile(clone: string, commit: string, file: string): string {
  return execFileSync('git', ['-C', clone, 'show', `${commit}:${file}`], {
    encoding: 'utf8',
  })
}

/** Writes `<setsDir>/thesvg/` from the clone of thesvg. */
export function prepareThesvg(clone: string, setsDir: string = SETS_DIR): void {
  const source = readJson(path.join(setsDir, 'thesvg.source.json'))
  if (!isRecord(source) || typeof source.commit !== 'string') {
    fail('thesvg.source.json has no commit')
  }
  const head = headOf(clone)
  if (head !== source.commit) {
    fail(`the clone is at ${head}, thesvg.source.json pins ${source.commit}`)
  }

  const fill = JSON.parse(
    readPinnedFile(clone, source.commit, 'src/data/ui/looks/fill.json'),
  ) as unknown
  const svgs = readStringMap(fill, 'fill.json')
  const kinds = readKinds(path.join(setsDir, 'kinds.json'))
  const table = readTable(path.join(setsDir, 'thesvg.kinds.json'))
  const colors = readStringMap(
    readJson(path.join(setsDir, 'thesvg.colors.json')),
    'thesvg.colors.json',
  )
  const problems = checkMapping({
    kinds,
    table,
    roles: ROLE_SLUGS,
    colors,
    slugs: new Set(Object.keys(svgs)),
  })
  if (problems.length > 0)
    fail(`the build inputs have problems:\n${problems.join('\n')}`)

  const license = readPinnedFile(clone, source.commit, 'LICENSE')
  if (!license.startsWith('MIT License'))
    fail('the LICENSE of the clone is not the MIT License')

  const outDir = path.join(setsDir, 'thesvg')
  freshDir(path.join(outDir, 'icons'))
  const iconDefinitions: Record<string, { iconPath: string }> = {}
  for (const slug of [...usedSlugs({ table, roles: ROLE_SLUGS })].sort()) {
    writeFileSync(
      path.join(outDir, 'icons', `${slug}.svg`),
      colorize(svgs[slug], colors[slug]),
    )
    iconDefinitions[slug] = { iconPath: `icons/${slug}.svg` }
  }

  const resolved = resolveKinds(kinds, table)
  const manifest = {
    iconDefinitions,
    file: ROLE_SLUGS.file,
    folder: ROLE_SLUGS.folder,
    fileExtensions: resolved.extensions,
    fileNames: resolved.names,
    folderNames: resolved.folders,
    'x-haex': { roles: { ...ROLE_SLUGS }, origin: ORIGIN },
  }
  writeJson(path.join(outDir, 'icon-theme.json'), manifest)
  writeFileSync(path.join(outDir, 'LICENSE'), license)
  writeFileSync(
    path.join(outDir, 'NOTICE'),
    `This pack contains the fill look of the UI icons of ${ORIGIN.source} (thesvg.org, MIT License, commit ${source.commit}). The drawings are by thesvg.org; they were coloured by kind when this set was built, nothing else was changed.\n`,
  )
  addStateIcons('thesvg', setsDir)
  console.log(
    `thesvg: ${Object.keys(iconDefinitions).length} icons, ${Object.keys(resolved.extensions).length} extensions, ${Object.keys(resolved.names).length} names, ${Object.keys(resolved.folders).length} folder names`,
  )
}
