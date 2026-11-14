// Removing the copy of a file another app shared to holzi once it is read (spec 092, FR-035). The
// copies are made by the Android side and held by the core until a vault is open
// (`shared_files.rs`); a copy is only a path inside the app's own cache.
import { invoke } from '@tauri-apps/api/core'
import type { PickedFile } from '@bindings/PickedFile'

/** The paths of copies the window has taken and that are not removed yet. */
const copies = new Set<string>()
/** Copies whose removal was refused; they are tried again at the next release. */
const failed = new Set<string>()

/** Remembers that `files` are copies of shared files, so reading one removes it. */
export function markShared(files: readonly PickedFile[]): void {
  for (const file of files) copies.add(file)
}

async function discardAsync(path: string): Promise<void> {
  try {
    await invoke('shared_files_discard', { path })
    copies.delete(path)
    failed.delete(path)
  } catch (error) {
    // Kept for a retry; the copy is in the app's cache, which the system clears too.
    failed.add(path)
    console.warn('[shared-files] a copy was not removed', error)
  }
}

/**
 * Removes the copy of a shared file; a file that is not one (a chosen file) is left alone. Called
 * as soon as the file was read, whatever came of it, and for files dropped unread (FR-035). A copy
 * that could not be removed stays pending and is tried again with the next one.
 */
export async function releaseSharedFile(file: PickedFile): Promise<void> {
  if (!copies.has(file)) return
  await discardAsync(file)
  for (const path of [...failed]) {
    if (path !== file) await discardAsync(path)
  }
}
