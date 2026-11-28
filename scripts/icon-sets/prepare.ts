// Spec 053 (T024): writes the source directories of the two bundled file icon
// sets, `src-tauri/icon-sets/phosphor/` and `src-tauri/icon-sets/material/` (the third one,
// `thesvg/`, is written by `thesvg.ts`, spec 103),
// from pinned upstream sources. The result is checked in, so every change to a
// bundled image shows up in a diff (research.md R6). Rust packs the directory
// into the ZIP; this script never writes a ZIP.
//
// Usage (in the devShell, from the repository root):
//
//   node scripts/icon-sets/prepare.ts phosphor <clone of phosphor-icons/core>
//   node scripts/icon-sets/prepare.ts material <unpacked Material Icon Theme VSIX> <Apache-2.0 text>
//   node scripts/icon-sets/prepare.ts thesvg <clone of glincker/thesvg at the pinned commit>
//
// The source paths come from the command line only, never from the script, so
// no local path ends up in the repository. Environment variables
// `ICON_SETS_PHOSPHOR`, `ICON_SETS_MATERIAL`, `ICON_SETS_APACHE` and `ICON_SETS_THESVG` stand in
// for the arguments.
//
// Pinned sources:
//   phosphor  phosphor-icons/core 2.1.1, weight `assets/regular`
//   material  Material Icon Theme 5.39.0, the released VSIX (`extension/...`)
//
// Convention for light/dark images (read by the loader, see SOURCES.md): the
// main part of a manifest is for the dark scheme, the block `light` for the
// light scheme. `x-haex.roles` names only the main ids. In the light scheme
// the loader takes the id `<id>-light` when `iconDefinitions` has it, else the
// role's own id. Phosphor writes every image twice (`<name>.svg` with light
// strokes, `<name>.light.svg` with dark strokes) and defines the second one
// as `<name>-light`.
import {
  copyFileSync,
  existsSync,
  readdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from 'node:fs'
import path from 'node:path'

import {
  addStateIcons,
  fail,
  freshDir,
  isRecord,
  mapValues,
  normalizeIconPath,
  pick,
  readJson,
  readString,
  readStringMap,
  SETS_DIR,
  writeJson,
} from './common.ts'
import type { Origin, Roles, StringMap } from './common.ts'
import {
  addKindFolderNames,
  readKinds,
  resolveKinds,
  tableProblems,
} from './kinds.ts'
import { prepareThesvg } from './thesvg.ts'

// Grey tones that stand in for `currentColor`; close to text-muted-foreground.
const STROKE_FOR_DARK_SCHEME = '#c9c9c9'
const STROKE_FOR_LIGHT_SCHEME = '#4a4a4a'

const PHOSPHOR_ORIGIN: Origin = {
  author: 'Phosphor Icons',
  license: 'MIT',
  source: 'https://github.com/phosphor-icons/core',
}

const MATERIAL_ORIGIN: Origin = {
  author: 'Material Extensions (Philipp Kief and contributors)',
  license: 'MIT AND Apache-2.0',
  source: 'https://github.com/material-extensions/vscode-material-icon-theme',
}

const MATERIAL_NOTICE =
  'This pack contains icons from Material Design Icons (Pictogrammers) and Material Symbols (Google), licensed under the Apache License 2.0. The logos and marks of companies, products, programming languages and tools shown in some icons belong to their owners and only mark the file type.\n'

// Phosphor roles, by image id (the file name without `.svg`).
const PHOSPHOR_ROLES: Roles = {
  folder: 'folder',
  folderNoAccess: 'folder-lock',
  file: 'file',
  image: 'file-image',
  video: 'file-video',
  audio: 'file-audio',
  pdf: 'file-pdf',
  text: 'file-text',
  encryptedFolder: 'folder-simple-lock',
  lockedFolder: 'folder-lock',
  lockedFile: 'file-lock',
  damaged: 'file-x',
}

// Material roles: only where an image of the right meaning exists. The states come from
// `haex-states/` (`addStateIcons`).
const MATERIAL_ROLES: Roles = {
  file: 'file',
  folder: 'folder',
  image: 'image',
  video: 'video',
  audio: 'audio',
  pdf: 'pdf',
  text: 'document',
}

/** Adds the localized names of the standard folders to the manifest of Material. */
function localizeMaterialFolders(): void {
  const manifestFile = path.join(SETS_DIR, 'material', 'icon-theme.json')
  const manifest = readJson(manifestFile)
  if (!isRecord(manifest)) fail('material: the manifest is not an object')
  const folderNames = readStringMap(manifest.folderNames, 'folderNames')
  const added = addKindFolderNames(
    readKinds(path.join(SETS_DIR, 'kinds.json')),
    folderNames,
    'material',
  )
  manifest.folderNames = folderNames
  writeJson(manifestFile, manifest)
  console.log(`material: ${added} localized folder names`)
}

function preparePhosphor(clone: string): void {
  const weightDir = path.join(clone, 'assets', 'regular')
  const names = readFileSync(path.join(SETS_DIR, 'phosphor.icons.txt'), 'utf8')
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line !== '' && !line.startsWith('#'))
  if (new Set(names).size !== names.length)
    fail('phosphor.icons.txt has duplicates')
  for (const name of names) {
    if (name.endsWith('-logo')) fail(`${name}: logo icons are never taken`)
    if (!existsSync(path.join(weightDir, `${name}.svg`))) {
      fail(`${name}.svg is not in the Phosphor clone`)
    }
  }

  const kinds = readKinds(path.join(SETS_DIR, 'kinds.json'))
  const table = readStringMap(
    (
      readJson(path.join(SETS_DIR, 'phosphor.kinds.json')) as {
        kinds?: unknown
      }
    ).kinds,
    'phosphor.kinds.json',
  )
  const problems = tableProblems(kinds, table, 'phosphor.kinds.json')
  for (const [kind, name] of Object.entries(table)) {
    if (!names.includes(name)) {
      problems.push(
        `kind ${kind} -> ${name}: ${name} is not in phosphor.icons.txt`,
      )
    }
  }
  if (problems.length > 0) fail(problems.join('\n'))
  const resolved = resolveKinds(kinds, table)
  for (const [role, name] of Object.entries(PHOSPHOR_ROLES)) {
    if (!names.includes(name))
      fail(`role ${role} -> ${name}: not in phosphor.icons.txt`)
  }

  const outDir = path.join(SETS_DIR, 'phosphor')
  freshDir(path.join(outDir, 'icons'))

  const iconDefinitions: Record<string, { iconPath: string }> = {}
  for (const name of names) {
    const svg = readFileSync(path.join(weightDir, `${name}.svg`), 'utf8')
    if (!svg.includes('currentColor')) fail(`${name}.svg has no currentColor`)
    writeFileSync(
      path.join(outDir, 'icons', `${name}.svg`),
      svg.replaceAll('currentColor', STROKE_FOR_DARK_SCHEME),
    )
    writeFileSync(
      path.join(outDir, 'icons', `${name}.light.svg`),
      svg.replaceAll('currentColor', STROKE_FOR_LIGHT_SCHEME),
    )
    iconDefinitions[name] = { iconPath: `icons/${name}.svg` }
    iconDefinitions[`${name}-light`] = { iconPath: `icons/${name}.light.svg` }
  }

  const manifest = {
    iconDefinitions,
    file: 'file',
    folder: 'folder',
    fileExtensions: resolved.extensions,
    fileNames: resolved.names,
    light: {
      file: 'file-light',
      folder: 'folder-light',
      fileExtensions: mapValues(resolved.extensions, (name) => `${name}-light`),
      fileNames: mapValues(resolved.names, (name) => `${name}-light`),
    },
    'x-haex': { roles: PHOSPHOR_ROLES, origin: PHOSPHOR_ORIGIN },
  }
  writeJson(path.join(outDir, 'icon-theme.json'), manifest)
  copyFileSync(path.join(clone, 'LICENSE'), path.join(outDir, 'LICENSE'))
  addStateIcons('phosphor')
  console.log(
    `phosphor: ${names.length} icons, ${names.length * 2} images, ${Object.keys(resolved.extensions).length} extensions, ${Object.keys(resolved.names).length} names`,
  )
}

function prepareMaterial(vsix: string, apacheText: string): void {
  const extensionDir = path.join(vsix, 'extension')
  const sourceIcons = path.join(extensionDir, 'icons')
  const source = readJson(
    path.join(extensionDir, 'dist', 'material-icons.json'),
  )
  if (!isRecord(source)) fail('material-icons.json is not an object')
  if (!isRecord(source.iconDefinitions))
    fail('material-icons.json has no iconDefinitions object')
  const fileNames = readStringMap(source.fileNames, 'fileNames')
  const fileExtensions = readStringMap(source.fileExtensions, 'fileExtensions')
  const folderNames = readStringMap(source.folderNames, 'folderNames')
  if (!isRecord(source.light)) fail('material-icons.json has no light object')

  const outDir = path.join(SETS_DIR, 'material')
  freshDir(path.join(outDir, 'icons'))

  const files = readdirSync(sourceIcons).filter((name) => name.endsWith('.svg'))
  files.sort()
  for (const name of files) {
    copyFileSync(path.join(sourceIcons, name), path.join(outDir, 'icons', name))
  }

  const iconDefinitions: Record<string, { iconPath: string }> = {}
  for (const [id, definition] of Object.entries(source.iconDefinitions)) {
    if (!isRecord(definition) || typeof definition.iconPath !== 'string') {
      fail(`${id}: definition without iconPath`)
    }
    const iconPath = normalizeIconPath(definition.iconPath)
    if (!existsSync(path.join(outDir, iconPath)))
      fail(`${id}: ${iconPath} is missing`)
    iconDefinitions[id] = { iconPath }
  }

  for (const [role, id] of Object.entries(MATERIAL_ROLES)) {
    if (!(id in iconDefinitions)) fail(`role ${role} -> ${id}: no such image`)
  }

  const maps = ['fileNames', 'fileExtensions', 'folderNames'] as const
  const manifest = {
    iconDefinitions,
    file: readString(source.file, 'file'),
    folder: readString(source.folder, 'folder'),
    fileNames,
    fileExtensions,
    folderNames,
    light: pick(source.light, ['file', 'folder', ...maps]),
    'x-haex': { roles: MATERIAL_ROLES, origin: MATERIAL_ORIGIN },
  }
  writeJson(path.join(outDir, 'icon-theme.json'), manifest)

  copyFileSync(
    path.join(extensionDir, 'LICENSE.txt'),
    path.join(outDir, 'LICENSE'),
  )
  copyFileSync(apacheText, path.join(outDir, 'LICENSE-APACHE-2.0'))
  writeFileSync(path.join(outDir, 'NOTICE'), MATERIAL_NOTICE)
  addStateIcons('material')
  localizeMaterialFolders()

  const manifestSize = statSync(path.join(outDir, 'icon-theme.json')).size
  const withStyle = files.filter((name) =>
    /<style[\s>]/i.test(readFileSync(path.join(outDir, 'icons', name), 'utf8')),
  ).length
  console.log(`material: ${files.length} images`)
  console.log(`material: manifest ${manifestSize} bytes`)
  console.log(
    `material: ${Object.keys(fileNames).length} fileNames, ${Object.keys(fileExtensions).length} fileExtensions, ${Object.keys(folderNames).length} folderNames`,
  )
  console.log(`material: ${withStyle} images with a <style> element`)
}

function main(): void {
  const [set, first, second] = process.argv.slice(2)
  if (set === 'phosphor') {
    const clone = first ?? process.env.ICON_SETS_PHOSPHOR
    if (!clone)
      fail('usage: prepare.ts phosphor <clone of phosphor-icons/core>')
    preparePhosphor(path.resolve(clone))
  } else if (set === 'material') {
    const vsix = first ?? process.env.ICON_SETS_MATERIAL
    const apache = second ?? process.env.ICON_SETS_APACHE
    if (!vsix || !apache) {
      fail(
        'usage: prepare.ts material <unpacked VSIX> <Apache-2.0 license text>',
      )
    }
    prepareMaterial(path.resolve(vsix), path.resolve(apache))
  } else if (set === 'thesvg') {
    const clone = first ?? process.env.ICON_SETS_THESVG
    if (!clone) fail('usage: prepare.ts thesvg <clone of glincker/thesvg>')
    prepareThesvg(path.resolve(clone))
  } else if (set === 'states') {
    // Adds the state images to the directories as they are, without the upstream sources.
    addStateIcons('phosphor')
    addStateIcons('material')
    addStateIcons('thesvg')
    localizeMaterialFolders()
  } else {
    fail(
      'usage: prepare.ts phosphor|material|thesvg <source> [<apache text>] | states',
    )
  }
}

try {
  main()
} catch (error) {
  console.error(`prepare.ts: ${error instanceof Error ? error.message : error}`)
  process.exit(1)
}
