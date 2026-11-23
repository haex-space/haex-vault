// The pages of the agents' shared memory as the window shows them (spec 097, "KI → Gedächtnis"):
// which tables they live in, how a refused write is named, and how `[[Title]]` in a page's text
// becomes a link. Pure, relative `.ts` imports only, so `scripts/check-agent-memory.ts` loads it.

/** The tables a view of the memory reloads on (`vault-data-changed`). */
export const AGENT_MEMORY_TABLES = ['haex_agent_memory_pages'] as const

/** The codes a write of the window can be refused with (Rust `MemoryError::reason`). */
const REFUSAL_CODES = [
  'empty',
  'line_break',
  'too_long',
  'invisible_characters',
  'secret',
  'instruction',
  'title_taken',
  'memory_full',
  'page_too_long',
  'not_found',
] as const

export type MemoryField = 'title' | 'description' | 'body'

/** The most characters of each field (Rust `agent_memory::limits`), shown as counters. */
export const MEMORY_FIELD_MAX: Record<MemoryField, number> = {
  title: 80,
  description: 200,
  body: 10_000,
}

/** Characters as the backend counts them (Unicode scalar values, not UTF-16 units). */
export function charCount(text: string): number {
  return [...text].length
}

/** A refused write: the language key of its reason and, for a refusal of a form, the field. */
export interface MemoryRefusal {
  key: string
  field?: MemoryField
}

/**
 * The refusal behind `error` (`InvalidInput` with `code` or `code:field`), or null for any other
 * failure, which the caller shows as it shows every error.
 */
export function memoryRefusal(error: unknown): MemoryRefusal | null {
  if (!error || typeof error !== 'object') return null
  const { kind, reason } = error as { kind?: unknown; reason?: unknown }
  if (kind !== 'InvalidInput' || typeof reason !== 'string') return null
  const [code, field] = reason.split(':')
  if (!(REFUSAL_CODES as readonly string[]).includes(code ?? '')) return null
  const key = `settings.agentMemory.refused.${code}`
  if (field === 'title' || field === 'description' || field === 'body') {
    return { key, field }
  }
  return { key }
}

/** A pointer of a page as the backend resolves it (Rust `AgentMemoryPointer`). */
export interface MemoryPointer {
  title: string
  id: string | null
}

function escapeHtml(text: string): string {
  return text
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;')
}

function titleKey(title: string): string {
  return title.trim().toLowerCase()
}

/** One `[[Title]]` or `[[Title|shown text]]` as HTML: a link to its page, or marked as missing. */
function pointerHtml(inner: string, ids: Map<string, string>): string | null {
  const [rawTitle = '', ...shownParts] = inner.split('|')
  const title = rawTitle.trim()
  if (!title) return null
  const shown = escapeHtml(shownParts.join('|').trim() || title)
  const id = ids.get(titleKey(title))
  if (id) {
    return `<a href="#" data-memory-page="${escapeHtml(id)}">${shown}</a>`
  }
  return `<span class="memory-missing" data-memory-missing="${escapeHtml(title)}">${shown}</span>`
}

function linkLine(line: string, ids: Map<string, string>): string {
  const linkPointers = (part: string) =>
    part.replace(/\[\[([^\]\n]*)\]\]/g, (whole, inner: string) => {
      return pointerHtml(inner, ids) ?? whole
    })
  const ticks = [...line.matchAll(/`+/g)]
  let output = ''
  let outsideStart = 0
  let cursor = 0
  while (cursor < ticks.length) {
    const opening = ticks[cursor]!
    const length = opening[0].length
    let closingIndex = cursor + 1
    while (closingIndex < ticks.length && ticks[closingIndex]![0].length !== length) closingIndex++
    if (closingIndex === ticks.length) {
      cursor++
      continue
    }
    const closing = ticks[closingIndex]!
    const openingAt = opening.index ?? 0
    const closingAt = closing.index ?? openingAt
    output += linkPointers(line.slice(outsideStart, openingAt))
    output += line.slice(openingAt, closingAt + closing[0].length)
    outsideStart = closingAt + closing[0].length
    cursor = closingIndex + 1
  }
  return output + linkPointers(line.slice(outsideStart))
}

/**
 * `markdown` with every pointer outside code turned into HTML, as the backend reads them
 * (`agent_memory::links`): a link carries the id of its page in `data-memory-page`, a pointer to
 * no page `data-memory-missing` with its title. The result is rendered with `marked` and sanitized.
 */
export function withPointerLinks(
  markdown: string,
  pointers: readonly MemoryPointer[],
): string {
  const ids = new Map<string, string>()
  for (const pointer of pointers) {
    if (pointer.id) ids.set(titleKey(pointer.title), pointer.id)
  }
  let fence: { marker: string; length: number } | null = null
  return markdown
    .split('\n')
    .map((line) => {
      const trimmed = line.trimStart()
      const marker = trimmed[0]
      let length = 0
      if (marker === '`' || marker === '~') {
        while (trimmed[length] === marker) length++
      }
      if (length >= 3) {
        if (fence && fence.marker === marker && length >= fence.length) fence = null
        else if (!fence) fence = { marker, length }
        return line
      }
      return fence ? line : linkLine(line, ids)
    })
    .join('\n')
}
