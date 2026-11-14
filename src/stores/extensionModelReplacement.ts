import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { defineStore } from 'pinia'
import type { ModelReplacementAnswer } from '@bindings/ModelReplacementAnswer'
import type { ModelReplacementRequest } from '@bindings/ModelReplacementRequest'

/**
 * The questions of a model job whose model could not take a step (spec 055): it failed while the
 * job ran, or the default model cannot take the step or is not available, which Rust finds before
 * it sends a request. Rust sends `model-replacement-request` and waits; holzi never picks another
 * model itself. The user names another model, or declines and the step ends with the failure of
 * the first one. One question is shown at a time; the job keeps waiting for each of them.
 */
export const useExtensionModelReplacementStore = defineStore(
  'extensionModelReplacement',
  () => {
    const queue = ref<ModelReplacementRequest[]>([])
    const shown = computed(() => queue.value[0])
    let unlisten: UnlistenFn[] | undefined

    async function startAsync(): Promise<void> {
      if (unlisten) return
      unlisten = []
      unlisten.push(
        await listen<ModelReplacementRequest>(
          'model-replacement-request',
          (event) => {
            queue.value = [...queue.value, event.payload]
          },
        ),
        // The job stopped waiting (cancelled, no answer in time): the question is moot.
        await listen<{ requestId: string }>(
          'model-replacement-ended',
          (event) => {
            queue.value = queue.value.filter(
              (q) => q.requestId !== event.payload.requestId,
            )
          },
        ),
      )
    }

    /** `answer` is `null` when the user declines. */
    async function answerAsync(
      requestId: string,
      answer: ModelReplacementAnswer | null,
    ): Promise<void> {
      queue.value = queue.value.filter((q) => q.requestId !== requestId)
      await invoke('model_replacement_answer', { requestId, answer })
    }

    return { queue, shown, startAsync, answerAsync }
  },
)
