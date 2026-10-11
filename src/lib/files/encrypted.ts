// Encrypted folders in the file browser (spec 048): where one may be created, and how an entry's
// encryption shows. Pure, so `scripts/check-files-encrypted.ts` runs it without vue.

import type { Entry } from '@bindings/Entry'
import type { SourceRef } from '@bindings/SourceRef'

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
export function encryptedLook(
  entry: Pick<Entry, 'kind' | 'encrypted'>,
): EncryptedLook | null {
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
