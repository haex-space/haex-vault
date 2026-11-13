// Password manager search (spec 034-password-manager, FR-007, research R11): filters the headers
// the window already holds. Exactly four things are searched — title, username, URL and the tag
// names — never a note, a password or any other field (Wallet's list adds the folder names of the
// pass, spec 092 FR-022, by giving its headers `folders`), so the search can never become a way to
// probe a secret. The words match fuzzily (Fuse.js) or as an abbreviation, and the hits come best
// first. Pure, so `scripts/check-passwords-search.ts` runs it without vue.
import Fuse from 'fuse.js'

import { fold } from '../search/fold.ts'

/** What the search looks at; an `ItemHeader` satisfies it, and so does nothing that carries more. */
export type SearchableHeader = {
  id: string
  title: string | null
  username: string | null
  url: string | null
  tags: { id: string; name: string }[]
  /** The names of the folders from the top down; only Wallet's list sets it (spec 092, FR-022). */
  folders?: string[]
}

const collator = new Intl.Collator()

/** The entries in the order of the list: by folded title, entries without a title last, then by
 * id. Each title is folded once, not in every comparison. */
export function sortByTitle<T extends { id: string; title: string | null }>(
  headers: readonly T[],
): T[] {
  const keyed = headers.map((header) => ({
    header,
    key: fold(header.title ?? ''),
  }))
  keyed.sort((a, b) => {
    if (!a.key !== !b.key) return a.key ? -1 : 1
    return (
      collator.compare(a.key, b.key) ||
      collator.compare(a.header.id, b.header.id)
    )
  })
  return keyed.map((entry) => entry.header)
}

/** The words of a query, folded; empty for a blank query. */
function terms(query: string): string[] {
  return fold(query)
    .split(/\s+/)
    .filter((word) => word.length > 0)
}

/** A reference placeholder (spec 036, `contracts/references.md`). The grammar itself lives in Rust
 * (`passwords/references.rs`); this only keeps the search from matching the raw token, so a search
 * for "password" or a piece of an id does not find every entry with a reference. */
const PLACEHOLDER =
  /\{\$[0-9A-Fa-f]{8}-(?:[0-9A-Fa-f]{4}-){3}[0-9A-Fa-f]{12}:(?:username|password|extra:(?:\\[\s\S]|[^}\\])+)\}/g

/** A text cut into plain parts and placeholders, for a list that shows a mark instead of the raw
 * token (spec 036, FR-047). Who the mark points at is left to the entry view, which asks Rust. */
export function placeholderParts(
  text: string,
): { kind: 'text' | 'mark'; text: string }[] {
  const parts: { kind: 'text' | 'mark'; text: string }[] = []
  let last = 0
  for (const match of text.matchAll(PLACEHOLDER)) {
    const start = match.index ?? 0
    if (start > last)
      parts.push({ kind: 'text', text: text.slice(last, start) })
    parts.push({ kind: 'mark', text: match[0] })
    last = start + match[0].length
  }
  if (last < text.length) parts.push({ kind: 'text', text: text.slice(last) })
  return parts
}

function withoutPlaceholders(text: string | null): string | null {
  return text === null ? null : text.replace(PLACEHOLDER, ' ')
}

/** How far a word may be from the text (Fuse.js: 0 is exact, 1 anything). At 0.3 a word of up to
 * three letters must match exactly, from four letters on one typo passes, from seven two. */
const FUZZY_THRESHOLD = 0.3

/** The score of a hit by abbreviation alone, so it ranks behind an exact or typo hit. */
const ABBREVIATION_SCORE = 0.5

/** The searched fields, as Fuse knows them. */
const KEYS = ['title', 'username', 'url', 'tags', 'folders']

/** The folded text of one searched field; the tags are a list. */
function fieldOf(header: SearchableHeader, key: string): string | string[] {
  switch (key) {
    case 'title':
      return fold(header.title ?? '')
    case 'username':
      return fold(withoutPlaceholders(header.username) ?? '')
    case 'url':
      return fold(withoutPlaceholders(header.url) ?? '')
    case 'folders':
      return (header.folders ?? []).map((name) => fold(name))
    default:
      return header.tags.map((tag) => fold(tag.name))
  }
}

/** True when the letters of the word appear in order in one word of a searched field, starting
 * with its first letter: "itms" finds "itemis", "gthb" finds "github". Fuse counts the left-out
 * letters as typos and rejects them. */
function abbreviates(word: string, header: SearchableHeader): boolean {
  return KEYS.flatMap((key) => fieldOf(header, key))
    .flatMap((text) => text.split(/[^\p{L}\p{N}]+/u))
    .some((token) => {
      if (token[0] !== word[0]) return false
      let next = 0
      for (const letter of token) if (letter === word[next]) next++
      return next >= word.length
    })
}

/** True when every word of the query matches one of the searched texts. A blank query matches
 * everything. */
export function matchesQuery(header: SearchableHeader, query: string): boolean {
  return filterHeaders([header], { query }).length > 0
}

/** The headers that match the query and, when given, carry the tag (`tagIds`: any of a group of
 * tags that show as one). Every word of the query has to match, each in any of the four fields;
 * with a query the hits come best first (ties keep the order of `headers`), without one in the
 * order of `headers`. */
export function filterHeaders<T extends SearchableHeader>(
  headers: readonly T[],
  options: { query: string; tagId?: string; tagIds?: readonly string[] },
): T[] {
  const words = terms(options.query)
  const wanted = options.tagIds ?? (options.tagId ? [options.tagId] : [])
  const tagged =
    wanted.length === 0
      ? [...headers]
      : headers.filter((header) =>
          header.tags.some((tag) => wanted.includes(tag.id)),
        )
  if (words.length === 0) return tagged
  // ponytail: the index is built for every query; at 5,000 entries a word takes 20 to 80 ms.
  // Upgrade: keep a `Fuse.createIndex` per list of headers and debounce the search field.
  const fuse = new Fuse(tagged, {
    keys: KEYS,
    getFn: (header, path) =>
      fieldOf(header, Array.isArray(path) ? (path[0] ?? '') : path),
    ignoreLocation: true,
    includeScore: true,
    threshold: FUZZY_THRESHOLD,
  })
  // The scores of the words add up; a header missing one word drops out.
  let scores = new Map<number, number>(tagged.map((_, index) => [index, 0]))
  for (const word of words) {
    const fuzzy = new Map(
      fuse.search(word).map((result) => [result.refIndex, result.score ?? 0]),
    )
    const next = new Map<number, number>()
    for (const [index, before] of scores) {
      const typo = fuzzy.get(index)
      const score =
        typo !== undefined && typo <= ABBREVIATION_SCORE
          ? typo
          : abbreviates(word, tagged[index]!)
            ? ABBREVIATION_SCORE
            : typo
      if (score !== undefined) next.set(index, before + score)
    }
    scores = next
    if (scores.size === 0) return []
  }
  return [...scores]
    .sort(([left, a], [right, b]) => a - b || left - right)
    .map(([index]) => tagged[index]!)
}
