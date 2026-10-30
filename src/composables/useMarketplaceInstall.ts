import { computed, ref } from 'vue'
import { Channel, invoke } from '@tauri-apps/api/core'
import type { CatalogDetail } from '@bindings/CatalogDetail'
import type { DownloadProgress } from '@bindings/DownloadProgress'
import type { PendingBundleRef } from '@bindings/PendingBundleRef'
import { isCancelled } from '~/lib/marketplace/errors'

/** What is being installed: shown while it downloads and named by the install dialog. */
export interface InstallTarget {
  sourceId: string
  slug: string
  /** The name the marketplace gives the entry; plain text. */
  name: string
  version: string
}

// One install at a time in the whole app (spec 069 FR-015): the state lives outside the
// component, so the catalog, the details and the frame that shows the progress and the dialog
// all see the same one.
const target = ref<InstallTarget | null>(null)
const progress = ref<DownloadProgress | null>(null)
const pendingId = ref<string | null>(null)
const downloading = ref(false)
const failure = ref<string | null>(null)
const publisherFingerprint = ref<string | undefined>(undefined)
let runningId: string | null = null
let generation = 0

/**
 * The install flow of the catalog (spec 069 US2): the host downloads and checks the bundle, the
 * progress comes through a channel, then the install dialog of spec 017 shows the preview of the
 * bundle the host holds. A second request while one runs is refused, never a second dialog.
 */
export function useMarketplaceInstall() {
  const { errString } = useErrorString()
  const { t } = useI18n()

  const busy = computed(() => downloading.value || pendingId.value !== null)
  const dialogOpen = computed({
    get: () => pendingId.value !== null,
    // The dialog closes by install or by the user; the host drops what is left.
    set: (open: boolean) => {
      if (!open) {
        const id = pendingId.value
        if (id) {
          // Capture the id before clearing shared state: the dialog's source prop disappears
          // with pendingId, so it cannot reliably clean up after this setter runs.
          void invoke('marketplace_download_discard', { id }).catch(() => {
            // The host already consumed or forgot the pending bundle.
          })
        }
        finish()
      }
    },
  })

  function finish() {
    pendingId.value = null
    target.value = null
    progress.value = null
    publisherFingerprint.value = undefined
  }

  /** Starts the download of `version`. Returns `false` when another install is running. */
  async function startAsync(next: InstallTarget): Promise<boolean> {
    if (busy.value) {
      failure.value = t('marketplace.progress.busy')
      return false
    }
    failure.value = null
    target.value = next
    progress.value = { loaded: 0, total: 0 }
    downloading.value = true
    const id = crypto.randomUUID()
    const mine = ++generation
    runningId = id
    const channel = new Channel<DownloadProgress>()
    channel.onmessage = (event) => {
      if (mine === generation) progress.value = event
    }
    // The key the marketplace lists for the entry: the dialog warns when the bundle's own
    // differs. Best effort, so a marketplace that does not answer the details does not stop it.
    const listed = invoke<CatalogDetail>('marketplace_details', {
      sourceId: next.sourceId,
      slug: next.slug,
    }).then(
      (detail) => detail.publisherFingerprint,
      () => undefined,
    )
    try {
      const ref = await invoke<PendingBundleRef>('marketplace_download', {
        sourceId: next.sourceId,
        slug: next.slug,
        version: next.version,
        id,
        onProgress: channel,
      })
      const publisher = await listed
      if (mine !== generation) {
        await invoke('marketplace_download_discard', { id: ref.id }).catch(
          () => undefined,
        )
        return false
      }
      publisherFingerprint.value = publisher
      pendingId.value = ref.id
      return true
    } catch (error) {
      if (mine !== generation) return false
      if (!isCancelled(error)) failure.value = errString(error)
      finish()
      return false
    } finally {
      if (mine === generation) {
        downloading.value = false
        runningId = null
      }
    }
  }

  async function cancelAsync(): Promise<void> {
    if (!runningId) return
    try {
      await invoke('marketplace_download_cancel', { id: runningId })
    } catch {
      // The download ended by itself in the meantime.
    }
  }

  /** Cancels and clears the shared install state when its owning app window closes. */
  async function disposeAsync(): Promise<void> {
    const cancelId = runningId
    const discardId = pendingId.value
    generation += 1
    runningId = null
    downloading.value = false
    failure.value = null
    finish()
    await Promise.all([
      ...(cancelId
        ? [invoke('marketplace_download_cancel', { id: cancelId })]
        : []),
      ...(discardId
        ? [invoke('marketplace_download_discard', { id: discardId })]
        : []),
    ]).catch(() => undefined)
  }

  return {
    target,
    progress,
    pendingId,
    downloading,
    busy,
    dialogOpen,
    failure,
    publisherFingerprint,
    startAsync,
    cancelAsync,
    disposeAsync,
    dismissFailure: () => {
      failure.value = null
    },
  }
}
