import { ref } from 'vue'
import { defineStore } from 'pinia'
import type { DownloadProgressEvent } from '~/composables/useModels'
import { progressPercent } from '~/lib/models/format'

/**
 * Progress of every running model download, by model id (spec 023-settings-app, research R6).
 * The settings views that start downloads read it here, so a download keeps showing its progress
 * after the user leaves its view and comes back.
 */
export const useModelDownloadsStore = defineStore('modelDownloads', () => {
  const models = useModels()
  const downloads = ref<Record<string, DownloadProgressEvent>>({})
  let watching: Promise<void> | null = null

  /**
   * Tracks progress and completion. Idempotent.
   *
   * ponytail: the subscription is never disposed — it lives until the process ends, which is the
   * end of the vault session (one vault per process, spec 013). A second vault in one process
   * would need an explicit stop here.
   */
  function watchDownloads(): Promise<void> {
    watching ??= Promise.all([
      models.onDownloadProgress((event) => {
        downloads.value = { ...downloads.value, [event.modelId]: event }
      }),
      models.onDownloadComplete((model) => {
        if (!downloads.value[model.id]) return
        downloads.value = {
          ...downloads.value,
          [model.id]: {
            modelId: model.id,
            bytesDownloaded: model.sizeBytes,
            bytesTotal: model.sizeBytes,
          },
        }
      }),
    ]).then(() => undefined)
    return watching
  }

  /** Forgets a download once its view has handled the result (done, failed or repaired). */
  function clearDownload(modelId: string) {
    if (!downloads.value[modelId]) return
    const { [modelId]: _removed, ...rest } = downloads.value
    downloads.value = rest
  }

  /** The progress of `modelId` while its download still runs; nothing once every byte is there.
   * A view may clear a download before its last progress event arrives, and speech model downloads
   * send no completion, so a finished download would otherwise stay on screen. */
  function running(modelId: string): DownloadProgressEvent | undefined {
    const state = downloads.value[modelId]
    if (!state) return undefined
    const done =
      state.bytesTotal !== null && state.bytesDownloaded >= state.bytesTotal
    return done ? undefined : state
  }

  function downloadPercent(modelId: string): number | null {
    const state = downloads.value[modelId]
    return state
      ? progressPercent(state.bytesDownloaded, state.bytesTotal)
      : null
  }

  return {
    downloads,
    watchDownloads,
    clearDownload,
    running,
    downloadPercent,
  }
})
