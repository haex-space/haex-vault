// Shared helpers of the icon set build scripts (`prepare.ts`, `thesvg.ts`): paths, small JSON and
// file helpers, and the state images that every bundled set carries (spec 053, spec 103).
import {
  copyFileSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import path from 'node:path'

export const ROOT = path.resolve(import.meta.dirname, '..', '..')
export const SETS_DIR = path.join(ROOT, 'src-tauri', 'icon-sets')

export type RoleKey =
  | 'folder'
  | 'folderNoAccess'
  | 'file'
  | 'image'
  | 'video'
  | 'audio'
  | 'pdf'
  | 'text'
  | 'encryptedFolder'
  | 'lockedFolder'
  | 'lockedFile'
  | 'damaged'
export type Roles = Partial<Record<RoleKey, string>>
export type StringMap = Record<string, string>

export type Origin = { author: string; license: string; source: string }

// haex-vault's own coloured images for the states of an entry (a readable encrypted folder, a folder of
// another vault, a folder without access, a locked file, a damaged one): the same in every
// bundled set, drawn to be told apart at a glance. They replace the role images of the set.
export const STATES_DIR = path.join(import.meta.dirname, 'haex-states')
export const STATE_IMAGES: Record<string, RoleKey> = {
  'encrypted-folder': 'encryptedFolder',
  'locked-folder': 'lockedFolder',
  'folder-no-access': 'folderNoAccess',
  'locked-file': 'lockedFile',
  damaged: 'damaged',
}

/** Adds the state images to the written source directory of a set and names them as its roles. */
export function addStateIcons(set: string, setsDir: string = SETS_DIR): void {
  const outDir = path.join(setsDir, set)
  const manifestFile = path.join(outDir, 'icon-theme.json')
  const manifest = readJson(manifestFile)
  if (!isRecord(manifest) || !isRecord(manifest.iconDefinitions))
    fail(`${set}: the manifest has no iconDefinitions object`)
  const haex = manifest['x-haex']
  if (!isRecord(haex) || !isRecord(haex.roles))
    fail(`${set}: the manifest has no x-haex.roles object`)
  for (const [name, role] of Object.entries(STATE_IMAGES)) {
    const id = `haex-${name}`
    copyFileSync(
      path.join(STATES_DIR, `${name}.svg`),
      path.join(outDir, 'icons', `${id}.svg`),
    )
    manifest.iconDefinitions[id] = { iconPath: `icons/${id}.svg` }
    haex.roles[role] = id
  }
  writeJson(manifestFile, manifest)
  console.log(`${set}: ${Object.keys(STATE_IMAGES).length} state images`)
}

/** A build error; the entry script prints it and exits, tests catch it. */
export function fail(message: string): never {
  throw new Error(message)
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

export function readJson(file: string): unknown {
  return JSON.parse(readFileSync(file, 'utf8'))
}

export function readString(value: unknown, label: string): string {
  if (typeof value !== 'string') fail(`${label} is not a string`)
  return value
}

export function readStringMap(value: unknown, label: string): StringMap {
  if (!isRecord(value)) fail(`${label} is not an object`)
  const entries = Object.entries(value)
  if (entries.some(([, entry]) => typeof entry !== 'string')) {
    fail(`${label} contains a non-string value`)
  }
  return Object.fromEntries(entries) as StringMap
}

export function writeJson(file: string, value: unknown): void {
  writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`)
}

export function freshDir(dir: string): void {
  rmSync(dir, { recursive: true, force: true })
  mkdirSync(dir, { recursive: true })
}

// `iconPath` is relative to the pack root, with `/`, no `./`, no `..`.
export function normalizeIconPath(raw: string): string {
  const normalized = path.posix.normalize(
    raw.replace(/^\.\/\.\.\//, '').replace(/^\.\//, ''),
  )
  if (
    normalized.startsWith('/') ||
    normalized.split('/').includes('..') ||
    normalized.startsWith('./')
  ) {
    fail(`icon path ${raw} does not normalize to a path inside the pack`)
  }
  return normalized
}

export function mapValues(
  map: StringMap,
  fn: (value: string) => string,
): StringMap {
  return Object.fromEntries(
    Object.entries(map).map(([key, value]) => [key, fn(value)]),
  )
}

export function pick<T extends object>(
  source: T,
  keys: readonly string[],
): Partial<T> {
  const picked: Record<string, unknown> = {}
  for (const key of keys) {
    if (key in source) picked[key] = (source as Record<string, unknown>)[key]
  }
  return picked as Partial<T>
}
