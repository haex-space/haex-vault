import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { defineStore } from 'pinia'
import {
  current,
  enqueue,
  remove,
  type PermissionQuestion,
} from '~/lib/extensions/queue'

/**
 * The permission questions of extensions (spec 017, US3, T069): Rust asks with
 * `extension-permission-request` and drops a question nobody waits for with
 * `extension-permission-request-cancelled`. One question is shown at a time.
 */
export const useExtensionPermissionsStore = defineStore(
  'extensionPermissions',
  () => {
    const queue = ref<PermissionQuestion[]>([])
    const shown = computed(() => current(queue.value))
    let unlisten: UnlistenFn[] = []

    async function startAsync(): Promise<void> {
      if (unlisten.length > 0) return
      unlisten = await Promise.all([
        listen<PermissionQuestion>('extension-permission-request', (event) => {
          queue.value = enqueue(queue.value, event.payload)
        }),
        listen<{ requestId: string }>(
          'extension-permission-request-cancelled',
          (event) => {
            queue.value = remove(queue.value, event.payload.requestId)
          },
        ),
      ])
    }

    /** `model` is the user's pick in a question about a model (spec 055 US2). */
    async function answerAsync(
      requestId: string,
      decision: 'allow' | 'deny',
      remember: boolean,
      model?: { modelId: string; effort: string },
    ): Promise<void> {
      queue.value = remove(queue.value, requestId)
      await invoke('extension_permission_resolve', {
        args: { requestId, decision, remember, model },
      })
    }

    /**
     * Closing the dialog cancels; it never denies. Rust drops the open question, so the next
     * identical call asks again.
     */
    function cancel(requestId: string): void {
      queue.value = remove(queue.value, requestId)
      void invoke('extension_permission_cancel', { requestId }).catch(
        (error: unknown) => {
          console.error('[extensions] cancelling the question failed', error)
        },
      )
    }

    return { queue, shown, startAsync, answerAsync, cancel }
  },
)
