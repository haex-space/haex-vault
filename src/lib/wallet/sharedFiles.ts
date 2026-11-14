// Removing the copy of a file another app shared to holzi once it is read (spec 092, FR-035). The
// copies are made by the Android side and held by the core until a vault is open
// (`shared_files.rs`); a copy is only a path inside the app's own cache.
import { invoke } from '@tauri-apps/api/core'
import type { PickedFile } from '@bindings/PickedFile'

/** The paths of copies the window has taken; only these are ever asked to be removed. */
const copies = new Set<string>()

/** Remembers that `files` are copies of shared files, so reading one removes it. */
export function markShared(files: readonly PickedFile[]): void {
  for (const file of files) copies.add(file)
}

/**
 * Removes the copy of a shared file; a file that is not one (a chosen file) is left alone. Called
 * as soon as the file was read, whatever came of it, and for files dropped unread (FR-035).
 */
export async function releaseSharedFile(file: PickedFile): Promise<void> {
  if (!copies.delete(file)) return
  try {
    await invoke('shared_files_discard', { path: file })
  } catch (error) {
    // The copy is in the app's cache, which the system clears too; nothing else to do.
    console.warn('[shared-files] a copy was not removed', error)
  }
}
