/**
 * The icons of the file browser (spec 053): which set the vault chose, the answers of Rust for the
 * entries that appeared, and the images as `blob:` URLs for an `<img>`. Rust decides what an entry
 * looks like (`files_icons_resolve`) and checks every image; here the questions of one frame go out
 * together and the results are kept in a bounded memory (`lib/files/fileIcons.ts`). A set or scheme
 * change, and any change of the sets in the vault, empties the memory and asks again.
 */
import { invoke } from '@tauri-apps/api/core'
import { shallowRef, watch, type Ref } from 'vue'
import type { IconAnswer } from '@bindings/IconAnswer'
import type { IconQuery } from '@bindings/IconQuery'
import type { IconSelection } from '@bindings/IconSelection'
import type { IconSetRef } from '@bindings/IconSetRef'
import type { Scheme } from '@bindings/Scheme'
import {
  ANSWER_MEMORY,
  batches,
  createMemory,
  IMAGE_MEMORY,
  imageKey,
  queryKey,
  selectionValue,
  STANDARD_SET,
} from '~/lib/files/fileIcons'

/** What an entry shows: an image of the set, or a symbol of the standard set. */
export type FileIconLook =
  { kind: 'image'; url: string } | { kind: 'builtin'; name: string }

type Asked = {
  key: string
  set: IconSetRef
  scheme: Scheme
  query: IconQuery
  generation: number
  done: (answer: IconAnswer | null) => void
}

/** One state per process: a process holds one vault (spec 013). */
const chosen = shallowRef<IconSetRef>(STANDARD_SET)
const generation = shallowRef(0)
const answers = createMemory<IconAnswer>(ANSWER_MEMORY)
const images = createMemory<string>(IMAGE_MEMORY, (url) =>
  URL.revokeObjectURL(url),
)
const fetching = new Map<string, Promise<string | null>>()
let queue: Asked[] = []
let flushing = false
let started: Promise<void> | null = null
let selectionKey = ''

function forget() {
  generation.value += 1
  answers.clear()
  images.clear()
  fetching.clear()
}

/** Reads the vault's choice; anything that changes what is answered empties the memory. */
async function refreshSelectionAsync(): Promise<void> {
  let next: IconSelection | null = null
  try {
    next = await invoke<IconSelection>('file_icon_set_selection')
  } catch {
    // No vault or a read error: the standard set answers.
  }
  const set = next?.set ?? STANDARD_SET
  const key = `${selectionValue(set)}|${next?.state ?? 'ready'}`
  if (key === selectionKey) return
  selectionKey = key
  chosen.value = set
  forget()
}

function start(): Promise<void> {
  started ??= (async () => {
    const scheme = useAppliedScheme()
    watch(scheme, forget)
    onVaultTablesChanged(['preferences', 'haex_file_icon_sets'], () =>
      refreshSelectionAsync(),
    )
    await refreshSelectionAsync()
  })()
  return started
}

function nextFrame(run: () => void) {
  if (typeof requestAnimationFrame === 'function') requestAnimationFrame(run)
  else setTimeout(run, 0)
}

/** Sends the questions that gathered since the last frame, one call per set and scheme. */
async function flushAsync() {
  flushing = false
  const asked = queue
  queue = []
  // Entries with the same key asked in one frame are sent once and share the answer.
  const waiting = new Map<string, Asked[]>()
  for (const item of asked)
    waiting.set(item.key, [...(waiting.get(item.key) ?? []), item])
  const bySetAndScheme = new Map<string, Asked[]>()
  for (const items of waiting.values()) {
    const [first] = items
    if (!first) continue
    const group = `${selectionValue(first.set)}|${first.scheme}`
    bySetAndScheme.set(group, [...(bySetAndScheme.get(group) ?? []), first])
  }
  for (const items of bySetAndScheme.values()) {
    await Promise.all(
      batches(items).map(async (group) => {
        const [head] = group
        if (!head) return
        const { set, scheme } = head
        let result: IconAnswer[] | null = null
        try {
          result = await invoke<IconAnswer[]>('files_icons_resolve', {
            set,
            scheme,
            queries: group.map((item) => item.query),
          })
        } catch {
          result = null
        }
        group.forEach((item, index) => {
          const answer = result?.[index] ?? null
          if (answer && item.generation === generation.value)
            answers.set(item.key, answer)
          for (const waiter of waiting.get(item.key) ?? []) waiter.done(answer)
        })
      }),
    )
  }
}

function ask(item: Omit<Asked, 'done'>): Promise<IconAnswer | null> {
  return new Promise((resolve) => {
    queue.push({ ...item, done: resolve })
    if (!flushing) {
      flushing = true
      nextFrame(() => void flushAsync())
    }
  })
}

/** The URL of an image of a set, fetched once; `null` when it cannot be read. */
async function imageUrlAsync(
  set: IconSetRef,
  answer: Extract<IconAnswer, { kind: 'image' }>,
  asked: number,
): Promise<string | null> {
  const key = imageKey(set, answer.id)
  const stored = images.get(key)
  if (stored) return stored
  let pending = fetching.get(key)
  if (!pending) {
    pending = (async () => {
      try {
        const bytes = await invoke<ArrayBuffer>('files_icon_image', {
          set,
          id: answer.id,
        })
        const url = URL.createObjectURL(
          new Blob([bytes], { type: answer.mime }),
        )
        if (asked !== generation.value) {
          URL.revokeObjectURL(url)
          return null
        }
        images.set(key, url)
        return url
      } catch {
        return null
      } finally {
        if (asked === generation.value) fetching.delete(key)
      }
    })()
    fetching.set(key, pending)
  }
  return await pending
}

function standardLook(query: IconQuery): FileIconLook {
  return {
    kind: 'builtin',
    name: query.isDir ? 'lucide:folder' : 'lucide:file',
  }
}

async function lookAsync(
  set: IconSetRef,
  scheme: Scheme,
  query: IconQuery,
): Promise<FileIconLook | null> {
  await start()
  const asked = generation.value
  const key = queryKey(set, scheme, query)
  const answer =
    answers.get(key) ??
    (await ask({ key, set, scheme, query, generation: asked }))
  if (asked !== generation.value) return null
  if (!answer) return standardLook(query)
  if (answer.kind === 'builtin') return { kind: 'builtin', name: answer.name }
  const url = await imageUrlAsync(set, answer, asked)
  if (asked !== generation.value) return null
  return url ? { kind: 'image', url } : standardLook(query)
}

/** The look of an entry when everything it needs is in memory already. */
function peek(
  set: IconSetRef,
  scheme: Scheme,
  query: IconQuery,
): FileIconLook | null {
  const answer = answers.get(queryKey(set, scheme, query))
  if (!answer) return null
  if (answer.kind === 'builtin') return { kind: 'builtin', name: answer.name }
  const url = images.get(imageKey(set, answer.id))
  return url ? { kind: 'image', url } : null
}

/**
 * The icon of one entry, reactive: `null` while it is pending (the row shows an empty box of the
 * same size), then an image or a symbol. `set` is the vault's choice unless the caller names one
 * (the preview in the settings shows every set).
 */
export function useFileIcon(
  query: () => IconQuery,
  set?: () => IconSetRef,
): Ref<FileIconLook | null> {
  const scheme = useAppliedScheme()
  const look = shallowRef<FileIconLook | null>(null)
  void start()
  watch(
    () =>
      [
        queryKey(set?.() ?? chosen.value, scheme.value, query()),
        generation.value,
      ] as const,
    async () => {
      const ofSet = set?.() ?? chosen.value
      const ofScheme = scheme.value
      const ofQuery = query()
      look.value = peek(ofSet, ofScheme, ofQuery)
      if (look.value) return
      const found = await lookAsync(ofSet, ofScheme, ofQuery)
      const current =
        queryKey(set?.() ?? chosen.value, scheme.value, query()) ===
        queryKey(ofSet, ofScheme, ofQuery)
      if (current && found) look.value = found
    },
    { immediate: true },
  )
  return look
}

/** Forgets everything and revokes every URL (the file browser has no frame left). */
export function clearFileIcons() {
  forget()
}
