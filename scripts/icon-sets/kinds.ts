// One mapping of file kinds for every bundled file icon set (spec 103, research R7).
//
// `src-tauri/icon-sets/kinds.json` says which extensions, file names and folder names belong to which
// kind of entry (`rust`, `pdf`, `folder-downloads` …), independent of any icon set. A set only says
// which of its own images stands for a kind (`<set>.kinds.json`, kind → image). A kind without an
// image of the set takes the image of its parent kind (`rust` → `code`); without one up the chain
// the entry gets the set's generic image. A new set therefore needs one small table, never a new
// list of extensions.
//
// Folder kinds are the ones whose id starts with `folder-`; their `names` are folder names (home,
// downloads, `Bilder` …), the `names` of every other kind are file names (`readme`, `.env`).
import path from 'node:path'

import { fail, isRecord, readJson } from './common.ts'
import type { StringMap } from './common.ts'

export type Kind = {
  parent?: string
  extensions: string[]
  names: string[]
}
export type Kinds = Record<string, Kind>

/** What a set gets from the shared mapping: name → image id, for the three manifest tables. */
export type Resolved = {
  extensions: StringMap
  names: StringMap
  folders: StringMap
}

export const FOLDER_PREFIX = 'folder-'

export function isFolderKind(id: string): boolean {
  return id.startsWith(FOLDER_PREFIX)
}

function readStringList(value: unknown, where: string): string[] {
  if (value === undefined) return []
  if (!Array.isArray(value) || value.some((item) => typeof item !== 'string')) {
    fail(`${where} is not a list of strings`)
  }
  return value as string[]
}

/** The shape of `kinds.json`; see `kindsProblems` for its rules. */
export function parseKinds(raw: unknown): Kinds {
  if (!isRecord(raw) || !isRecord(raw.kinds)) {
    fail('kinds.json has no kinds object')
  }
  const kinds: Kinds = {}
  for (const [id, value] of Object.entries(raw.kinds)) {
    if (!isRecord(value)) fail(`kind ${id} is not an object`)
    if (value.parent !== undefined && typeof value.parent !== 'string') {
      fail(`kind ${id}: parent is not a string`)
    }
    kinds[id] = {
      ...(value.parent === undefined ? {} : { parent: value.parent }),
      extensions: readStringList(value.extensions, `kind ${id} extensions`),
      names: readStringList(value.names, `kind ${id} names`),
    }
  }
  return kinds
}

/** The problems of the kinds (every name in lower case, every extension and name once, a parent chain without cycles); empty when fine. */
export function kindsProblems(kinds: Kinds): string[] {
  const problems: string[] = []
  const owner = new Map<string, string>()
  const claim = (space: string, key: string, id: string): void => {
    if (key !== key.toLowerCase())
      problems.push(`${id}: ${key} must be in lower case`)
    const slot = `${space}:${key}`
    const first = owner.get(slot)
    if (first !== undefined)
      problems.push(`${space} ${key} is in ${first} and in ${id}`)
    else owner.set(slot, id)
  }
  for (const [id, kind] of Object.entries(kinds)) {
    if (id !== id.toLowerCase())
      problems.push(`${id}: the id must be lower case`)
    for (const extension of kind.extensions) {
      if (extension.startsWith('.'))
        problems.push(`${id}: ${extension} is written without its dot`)
      claim('extension', extension, id)
    }
    // A file name and a folder name are different namespaces.
    for (const name of kind.names)
      claim(isFolderKind(id) ? 'folder' : 'name', name, id)
    if (isFolderKind(id)) {
      if (kind.parent !== undefined)
        problems.push(`${id}: a folder kind has no parent`)
      if (kind.extensions.length > 0)
        problems.push(`${id}: a folder kind has no extensions`)
    } else if (kind.parent !== undefined) {
      if (!(kind.parent in kinds))
        problems.push(`${id}: the parent ${kind.parent} does not exist`)
      else if (isFolderKind(kind.parent))
        problems.push(`${id}: the parent is a folder kind`)
    }
    const seen = new Set<string>()
    for (
      let at: string | undefined = id;
      at !== undefined;
      at = kinds[at]?.parent
    ) {
      if (seen.has(at)) {
        problems.push(`${id}: the parent chain loops at ${at}`)
        break
      }
      seen.add(at)
    }
  }
  return problems
}

export function readKinds(file: string): Kinds {
  const kinds = parseKinds(readJson(file))
  const problems = kindsProblems(kinds)
  if (problems.length > 0)
    fail(`${path.basename(file)} has problems:\n${problems.join('\n')}`)
  return kinds
}

/** The kinds a set's table names that `kinds.json` does not have. */
export function tableProblems(
  kinds: Kinds,
  table: StringMap,
  label: string,
): string[] {
  return Object.keys(table)
    .filter((id) => !(id in kinds))
    .sort()
    .map((id) => `${label}: ${id} is not a kind of kinds.json`)
}

/** The image of a kind in a set: its own, else the one of the nearest parent that has one. */
export function imageOf(
  kinds: Kinds,
  table: StringMap,
  id: string,
): string | undefined {
  for (
    let at: string | undefined = id;
    at !== undefined;
    at = isFolderKind(at) ? undefined : kinds[at]?.parent
  ) {
    if (table[at] !== undefined) return table[at]
  }
  return undefined
}

/** Maps every extension, file name and folder name to the image of its kind in a set. */
export function resolveKinds(kinds: Kinds, table: StringMap): Resolved {
  const resolved: Resolved = { extensions: {}, names: {}, folders: {} }
  for (const [id, kind] of Object.entries(kinds)) {
    const image = imageOf(kinds, table, id)
    if (image === undefined) continue
    for (const extension of kind.extensions)
      resolved.extensions[extension] = image
    const into = isFolderKind(id) ? resolved.folders : resolved.names
    for (const name of kind.names) into[name] = image
  }
  return resolved
}

/**
 * For a set that brings its own mapping (Material): gives every name of a folder kind the image the
 * set already has for one of the kind's names, and returns how many names it added. A name the set
 * maps itself stays as it is; a folder kind none of whose names the set knows stops the build.
 */
export function addKindFolderNames(
  kinds: Kinds,
  folderNames: StringMap,
  set: string,
): number {
  let added = 0
  for (const [id, kind] of Object.entries(kinds)) {
    if (!isFolderKind(id)) continue
    const known = kind.names.find((name) => name in folderNames)
    if (known === undefined) fail(`${set} has no folder image for ${id}`)
    for (const name of kind.names) {
      if (name in folderNames) continue
      folderNames[name] = folderNames[known]
      added += 1
    }
  }
  return added
}
