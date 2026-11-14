import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { SharedFileView } from '@bindings/SharedFileView'
import { markShared } from '~/lib/wallet/sharedFiles'
import { routeShared } from '~/lib/wallet/sharedRoute'

/** Sent by Rust when files wait (`shared_files::EVENT`). */
const CHANGED_EVENT = 'shared-files-changed'

let started = false

/**
 * Files other apps share to holzi (spec 092, FR-037): a pass file, a PDF or a picture. The core
 * holds the copies while no vault is open; this is started once a vault is, takes what waits and
 * what comes later, puts the files in the queue of their dialog and opens Wallet so the dialog
 * shows. Nothing is imported or saved from here: the person confirms in the dialog.
 */
export function useSharedFile() {
  const imports = useWalletImportStore()
  const captures = useWalletCaptureStore()

  async function takeAsync(
    openWalletAsync: () => Promise<void>,
  ): Promise<void> {
    const waiting = await invoke<SharedFileView[]>('shared_files_take')
    if (waiting.length === 0) return
    const routed = routeShared(waiting)
    markShared([...routed.imports, ...routed.captures.map((c) => c.file)])
    imports.enqueue(routed.imports)
    captures.enqueue(routed.captures)
    await openWalletAsync()
  }

  async function startAsync(
    openWalletAsync: () => Promise<void>,
  ): Promise<void> {
    if (started) return
    started = true
    try {
      await listen(CHANGED_EVENT, () => {
        void takeAsync(openWalletAsync).catch((error: unknown) => {
          console.error('[shared-files] taking shared files failed', error)
        })
      })
      await takeAsync(openWalletAsync)
    } catch (error) {
      // Outside the app (a browser during development) there is nothing to listen to.
      console.error('[shared-files] listening failed', error)
    }
  }

  return { startAsync }
}
