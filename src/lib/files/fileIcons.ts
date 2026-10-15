// The pure part of the file icons (spec 053): the key of an answer, the grouping of the questions
// of one frame, and the bounded memory of answers and images. No vue, so that
// `scripts/check-file-icons.ts` runs it with `node --test`. What a name or an extension looks like
// is decided in Rust (`files::icons::resolve`); nothing here looks at a name.

/** What the bindings `IconSetRef`, `Scheme` and `IconQuery` carry, restated because the generated
 * files import without file extensions, which `node --test` and the scripts' typecheck cannot load. */
export type IconSetRef =
  | { kind: 'builtin'; name: 'lucide' | 'phosphor' | 'material' }
  | { kind: 'custom'; id: string }
export type Scheme = 'light' | 'dark'
export type IconQuery = {
  name: string
  isDir: boolean
  noAccess: boolean
  encrypted: { role: string; state: string; foreign?: number } | null
}

/** How many entries one call of `files_icons_resolve` may ask about (the limit of Rust). */
export const MAX_QUERIES = 500
/** How many answers and how many images the window keeps; the oldest leave first. */
export const ANSWER_MEMORY = 4000
export const IMAGE_MEMORY = 600

/** The set a vault has until it chooses another. */
export const DEFAULT_SET: IconSetRef = { kind: 'builtin', name: 'material' }

/** The standard set (Lucide): it answers where nothing can be asked (no vault, a read error). */
export const FALLBACK_SET: IconSetRef = { kind: 'builtin', name: 'lucide' }

/** The value of the vault preference `files.iconSet` for a set (`is_valid_selection` in Rust). */
export function selectionValue(set: IconSetRef): string {
  return set.kind === 'builtin' ? `builtin:${set.name}` : `custom:${set.id}`
}

/** One answer per `(set, scheme, name, isDir, noAccess, encrypted)`; the foreign-file count of an
 * encrypted folder does not change its icon and stays out. */
export function queryKey(
  set: IconSetRef,
  scheme: Scheme,
  query: IconQuery,
): string {
  const encrypted = query.encrypted
    ? `${query.encrypted.role}:${query.encrypted.state}`
    : ''
  return [
    selectionValue(set),
    scheme,
    query.isDir ? 'dir' : 'file',
    query.noAccess ? 'noaccess' : '',
    encrypted,
    query.name,
  ].join('|')
}

/** The key of an image of a set: the id is the path in the pack, which differs per scheme. */
export function imageKey(set: IconSetRef, id: string): string {
  return `${selectionValue(set)}|${id}`
}

/** The entries of one frame in calls of at most `max`, each key asked once. */
export function batches<T extends { key: string }>(
  asked: readonly T[],
  max: number = MAX_QUERIES,
): T[][] {
  const seen = new Set<string>()
  const groups: T[][] = []
  for (const item of asked) {
    if (seen.has(item.key)) continue
    seen.add(item.key)
    const last = groups[groups.length - 1]
    if (last && last.length < max) last.push(item)
    else groups.push([item])
  }
  return groups
}

export interface Memory<V> {
  get: (key: string) => V | undefined
  set: (key: string, value: V) => void
  /** Forgets everything; `onLeave` runs for every value. */
  clear: () => void
  size: () => number
}

/** A map of at most `capacity` values; a read makes a value the newest, the oldest leave first. */
export function createMemory<V>(
  capacity: number,
  onLeave?: (value: V) => void,
): Memory<V> {
  const stored = new Map<string, V>()
  return {
    get(key) {
      const value = stored.get(key)
      if (value === undefined) return undefined
      stored.delete(key)
      stored.set(key, value)
      return value
    },
    set(key, value) {
      const previous = stored.get(key)
      if (previous !== undefined && previous !== value) onLeave?.(previous)
      stored.delete(key)
      stored.set(key, value)
      while (stored.size > capacity) {
        const [oldest, leaving] = stored.entries().next().value as [string, V]
        stored.delete(oldest)
        onLeave?.(leaving)
      }
    },
    clear() {
      for (const value of stored.values()) onLeave?.(value)
      stored.clear()
    },
    size: () => stored.size,
  }
}
