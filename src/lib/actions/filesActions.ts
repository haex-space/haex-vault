// File actions for agents (spec 044 US6, contracts/agent-actions.md, ADR 0011). All but
// `files.show` run in Rust without a window (`runner: 'native'`, `src-tauri/src/files/agent/`);
// Rust fixes the caller, keeps holzi's own data closed and asks the user for a storage.
// `files.show` opens the file browser and runs here (`stores/filesActionHandlers.ts`). Pure,
// relative `.ts` imports only.
import type { ActionDefinition, JsonSchema } from './types.ts'

const SOURCE: JsonSchema = {
  type: 'string',
  description:
    '"device" for the files of this device, "storage:<id>" or "storage:<name>" for a storage (see files_sources).',
}

const PATH: JsonSchema = {
  type: 'string',
  description:
    'On the device an absolute path; in a storage a path from "/", such as "/Fotos/2025".',
}

const PATHS: JsonSchema = {
  type: 'array',
  items: PATH,
  description: 'The files and folders, by path.',
}

const ENTRY: JsonSchema = {
  type: 'object',
  properties: {
    name: { type: 'string' },
    path: { type: 'string' },
    kind: { type: 'string', enum: ['file', 'dir'] },
    size: { type: 'integer', description: 'Bytes; files only.' },
    modified: { type: 'string', description: 'ISO time of the last change.' },
  },
}

const ENTRIES: JsonSchema = { type: 'array', items: ENTRY }

const SUMMARY: JsonSchema = {
  type: 'object',
  properties: {
    items: { type: 'integer' },
    skipped: { type: 'integer' },
    keptBoth: { type: 'integer' },
    replaced: { type: 'integer' },
  },
}

const TRANSFER_INPUT = (conflicts: readonly string[]): JsonSchema => ({
  type: 'object',
  properties: {
    source: SOURCE,
    paths: PATHS,
    to: { type: 'string', description: 'The folder they go into.' },
    toSource: {
      type: 'string',
      description:
        'Where `to` lies, as `source`; the same as `source` when left out.',
    },
    onConflict: {
      type: 'string',
      enum: conflicts,
      description: 'What happens to a name already taken at the target.',
    },
  },
  required: ['source', 'paths', 'to'],
})

/** The fields every native action shares. */
const NATIVE = {
  target: 'none',
  agentCallable: true,
  binding: 'global',
  runner: 'native',
} as const

export const FILES_ACTIONS: readonly ActionDefinition[] = [
  {
    id: 'files.sources',
    titleKey: 'actions.files.sources',
    description:
      'Where files can be reached: the drives and known folders of this device (source "device") and the storages you have access to (source "storage:<id>"). Call it first to learn where to list or search. A storage the user names but that is not listed can be used as "storage:<name>"; holzi asks the user then.',
    input: { type: 'object' },
    result: {
      type: 'object',
      properties: {
        device: { type: 'object' },
        storages: { type: 'array', items: { type: 'object' } },
      },
    },
    scope: 'files.read',
    effect: 'read',
    ...NATIVE,
  },
  {
    id: 'files.list',
    titleKey: 'actions.files.list',
    description:
      'List the entries of a folder: name, path, kind (file or dir), size in bytes and the time of the last change. At most `limit` entries (500 by default); `truncated` says there are more.',
    input: {
      type: 'object',
      properties: {
        source: SOURCE,
        path: PATH,
        limit: { type: 'integer', description: 'At most 2000.' },
      },
      required: ['source', 'path'],
    },
    result: {
      type: 'object',
      properties: { entries: ENTRIES, truncated: { type: 'boolean' } },
    },
    scope: 'files.read',
    effect: 'read',
    ...NATIVE,
  },
  {
    id: 'files.stat',
    titleKey: 'actions.files.stat',
    description: 'Details of one file or folder.',
    input: {
      type: 'object',
      properties: { source: SOURCE, path: PATH },
      required: ['source', 'path'],
    },
    result: ENTRY,
    scope: 'files.read',
    effect: 'read',
    ...NATIVE,
  },
  {
    id: 'files.search',
    titleKey: 'actions.files.search',
    description:
      'Search a folder and everything below it for file and folder names like `query`; small typos still match. Filters: `types`, `sizeMin`/`sizeMax` in bytes, `modifiedAfter`/`modifiedBefore` as YYYY-MM-DD. At most 500 hits, best first; the search stops after 30 seconds, and `truncated` says it did not finish. Hidden files are left out.',
    input: {
      type: 'object',
      properties: {
        source: SOURCE,
        path: { ...PATH, description: `Where to start. ${PATH.description}` },
        query: { type: 'string', description: 'Part of the name.' },
        types: {
          type: 'array',
          items: {
            type: 'string',
            enum: ['image', 'video', 'audio', 'document', 'text'],
          },
        },
        sizeMin: { type: 'integer' },
        sizeMax: { type: 'integer' },
        modifiedAfter: { type: 'string' },
        modifiedBefore: { type: 'string' },
      },
      required: ['source', 'path', 'query'],
    },
    result: {
      type: 'object',
      properties: { hits: ENTRIES, truncated: { type: 'boolean' } },
    },
    scope: 'files.read',
    effect: 'read',
    ...NATIVE,
  },
  {
    id: 'files.read',
    titleKey: 'actions.files.read',
    description:
      'Read a file: the text of a text file, a PDF, a Word or Writer document (docx, odt) or a spreadsheet (xlsx, xls, ods; one block per sheet under its name), at most 200 000 characters (`truncated` says the rest was cut). An image is shown to you if your model accepts images. For anything else, a scanned PDF or a file over 50 MB you get the details of the file and a `note` why there is no text.',
    input: {
      type: 'object',
      properties: { source: SOURCE, path: PATH },
      required: ['source', 'path'],
    },
    result: {
      type: 'object',
      properties: {
        entry: ENTRY,
        text: { type: 'string' },
        truncated: { type: 'boolean' },
        note: { type: 'string' },
      },
    },
    scope: 'files.read',
    effect: 'read',
    ...NATIVE,
  },
  {
    id: 'files.folder.create',
    titleKey: 'actions.files.folder.create',
    description: 'Create a folder named `name` in the folder `path`.',
    input: {
      type: 'object',
      properties: {
        source: SOURCE,
        path: {
          ...PATH,
          description: `The parent folder. ${PATH.description}`,
        },
        name: { type: 'string' },
      },
      required: ['source', 'path', 'name'],
    },
    result: ENTRY,
    scope: 'files.write',
    effect: 'write',
    ...NATIVE,
  },
  {
    id: 'files.copy',
    titleKey: 'actions.files.copy',
    description:
      'Copy files and folders into the folder `to`, on the same source or onto another one (`toSource`, such as from the device into a storage). A name already taken at the target is skipped ("skip", the default), or the copy gets a free name such as "name (2).txt" ("keepBoth"). A copy never replaces anything.',
    input: TRANSFER_INPUT(['skip', 'keepBoth']),
    result: SUMMARY,
    scope: 'files.write',
    effect: 'write',
    ...NATIVE,
  },
  {
    id: 'files.rename',
    titleKey: 'actions.files.rename',
    description: 'Rename a file or folder; `newName` is a name, not a path.',
    input: {
      type: 'object',
      properties: { source: SOURCE, path: PATH, newName: { type: 'string' } },
      required: ['source', 'path', 'newName'],
    },
    result: ENTRY,
    scope: 'files.write',
    effect: 'write',
    ...NATIVE,
  },
  {
    id: 'files.move',
    titleKey: 'actions.files.move',
    description:
      'Move files and folders into the folder `to`, on the same source or onto another one (`toSource`). A name already taken at the target is skipped ("skip", the default), gets a free name ("keepBoth"), or is replaced ("replace").',
    input: TRANSFER_INPUT(['skip', 'keepBoth', 'replace']),
    result: SUMMARY,
    scope: 'files.write',
    effect: 'destructive',
    ...NATIVE,
  },
  {
    id: 'files.delete',
    titleKey: 'actions.files.delete',
    description:
      'Delete files and folders. On a computer they go to the trash of the system (`trash` is true); on a phone and in a storage they are gone for good.',
    input: {
      type: 'object',
      properties: { source: SOURCE, paths: PATHS },
      required: ['source', 'paths'],
    },
    result: {
      type: 'object',
      properties: {
        deleted: { type: 'integer' },
        trash: { type: 'boolean' },
      },
    },
    scope: 'files.write',
    effect: 'destructive',
    ...NATIVE,
  },
  {
    id: 'files.show',
    titleKey: 'actions.files.show',
    description:
      'Show a file or folder to the user: opens the file browser in the folder, or with the file open in its viewer (videos and audio play there).',
    input: {
      type: 'object',
      properties: { source: SOURCE, path: PATH },
      required: ['source', 'path'],
    },
    result: { type: 'object', properties: { opened: { type: 'boolean' } } },
    target: 'none',
    scope: 'files.write',
    effect: 'write',
    agentCallable: true,
    binding: 'global',
  },
]
