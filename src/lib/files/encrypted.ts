// Encrypted folders in the file browser (spec 048): where one may be created, and how an entry's
// encryption shows. Pure, so `scripts/check-files-encrypted.ts` runs it without vue.

import type { SourceRef } from '../../types/bindings/SourceRef.ts'

import { isPeerStorage } from './registry.ts'

/** What `Entry` carries about encryption (bindings `EncryptedInfo`); restated here because the
 * generated `Entry` imports without file extensions, which `node --test` cannot load. */
export type EncryptedEntry = {
  kind: 'file' | 'dir'
  encrypted: {
    role: 'folder' | 'inside'
    state: 'readable' | 'otherVault' | 'newerFormat' | 'damaged'
    /** Files in the folder that are not part of the format (research R19); the folder itself. */
    foreign?: number
  } | null
}

/** The prefixes of extension areas in a storage (spec 038 FR-010); no encrypted folder goes there. */
const EXTENSION_AREAS = ['/holzi-ext/', '/holzi-ext-dev/']

export type EncryptHereInput = {
  source: SourceRef
  /** The open folder's path as the window sees it. */
  path: string | null
  /** The open folder is, or lies in, an encrypted folder: nothing new is encrypted there. */
  inEncrypted: boolean
  /** The open folder is one of holzi's own places (FR-037): nothing is created there. */
  owned?: boolean
}

/**
 * Whether "Neuer verschlüsselter Ordner" belongs in the open folder: in a storage (FR-001) and in
 * any folder of this device (US7, FR-043), but not inside an encrypted folder, not in an extension
 * area, not in holzi's own places and not on another own device.
 */
export function canCreateEncrypted(input: EncryptHereInput): boolean {
  if (input.path === null || input.inEncrypted || input.owned) return false
  if (input.source.kind === 'device') return true
  // Another own device keeps its encrypted folders itself (spec 056).
  if (isPeerStorage(input.source.storageId)) return false
  const folder = input.path.endsWith('/') ? input.path : `${input.path}/`
  return !EXTENSION_AREAS.some(
    (area) => folder === area || folder.startsWith(area),
  )
}

export type DeleteInput = {
  source: SourceRef
  android: boolean
  /** The open folder is, or lies in, an encrypted folder. */
  inEncrypted: boolean
  entries: readonly EncryptedEntry[]
}

/**
 * Whether deleting asks first because it is for good (spec 044 FR-023, spec 048 FR-046): on
 * Android, in a storage, and in or of an encrypted folder, which has no trash.
 */
export function asksBeforeDelete(input: DeleteInput): boolean {
  return (
    input.android ||
    input.source.kind === 'storage' ||
    input.inEncrypted ||
    input.entries.some((entry) => entry.encrypted !== null)
  )
}

export type EncryptedLook = {
  /** i18n key of the state, when it is worth a word (not for a readable entry). */
  stateKey: string | null
}

/**
 * The state text of an entry with encryption, or `null` for an ordinary entry. Which icon the state
 * gets is the icon set's to say (spec 053, `files::icons`), not this module's.
 */
export function encryptedLook(entry: EncryptedEntry): EncryptedLook | null {
  const info = entry.encrypted
  if (!info) return null
  if (info.state !== 'readable')
    return { stateKey: `files.encrypted.state.${info.state}` }
  if (info.role === 'folder') return { stateKey: null }
  return null
}

/**
 * The i18n key of the name to show instead of `entry.name`, or `null` to show the name. A folder
 * whose head cannot be read keeps its random prefix as name (`….hxef`), and a sidecar that does
 * not open keeps its id: neither means anything to a person.
 */
export function shownNameKey(entry: EncryptedEntry): string | null {
  const info = entry.encrypted
  if (!info || info.state === 'readable') return null
  if (info.role === 'folder') return `files.encrypted.state.${info.state}`
  return 'files.encrypted.damagedEntry'
}
