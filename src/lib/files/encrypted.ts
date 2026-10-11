// Encrypted folders in the file browser (spec 048): where one may be created, and how an entry's
// encryption shows. Pure, so `scripts/check-files-encrypted.ts` runs it without vue.

import type { SourceRef } from '../../types/bindings/SourceRef.ts'

/** What `Entry` carries about encryption (bindings `EncryptedInfo`); restated here because the
 * generated `Entry` imports without file extensions, which `node --test` cannot load. */
export type EncryptedEntry = {
  kind: 'file' | 'dir'
  encrypted: {
    role: 'folder' | 'inside'
    state: 'readable' | 'otherVault' | 'newerFormat' | 'damaged'
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
}

/** Whether "Neuer verschlüsselter Ordner" belongs in the open folder (FR-001). */
export function canCreateEncrypted(input: EncryptHereInput): boolean {
  if (input.source.kind !== 'storage' || input.path === null) return false
  if (input.inEncrypted) return false
  const folder = input.path.endsWith('/') ? input.path : `${input.path}/`
  return !EXTENSION_AREAS.some(
    (area) => folder === area || folder.startsWith(area),
  )
}

export type EncryptedLook = {
  icon: string
  /** i18n key of the state, when it is worth a word (not for a readable entry). */
  stateKey: string | null
}

/** The icon and state text of an entry with encryption, or `null` for an ordinary entry. */
export function encryptedLook(entry: EncryptedEntry): EncryptedLook | null {
  const info = entry.encrypted
  if (!info) return null
  if (info.state !== 'readable') {
    return {
      icon:
        info.state === 'damaged'
          ? 'lucide:file-warning'
          : entry.kind === 'dir'
            ? 'lucide:folder-lock'
            : 'lucide:file-lock',
      stateKey: `files.encrypted.state.${info.state}`,
    }
  }
  if (info.role === 'folder')
    return { icon: 'lucide:folder-key', stateKey: null }
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
