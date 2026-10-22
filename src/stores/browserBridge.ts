import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { defineStore } from 'pinia'
import type { BridgeAnswer } from '~/types/bindings/BridgeAnswer'
import type { BridgePromptEvent } from '~/types/bindings/BridgePromptEvent'

/**
 * The questions of the External Bridge (spec 049, research R10, contracts/tauri-commands.md): Rust
 * asks with `browser-bridge-prompt` and closes a question nobody waits for any more with
 * `browser-bridge-prompt-cancelled`. One question is shown at a time; holzi comes to the front for
 * it. Only this window answers; no client, extension or agent can (FR-013).
 */
export const useBrowserBridgeStore = defineStore('browserBridge', () => {
  const queue = ref<BridgePromptEvent[]>([])
  const shown = computed(() => queue.value[0])
  /** Counts `browser-bridge-changed`, so the settings reload when it moves. */
  const changes = ref(0)
  let unlisten: UnlistenFn[] = []

  function remove(promptId: string): void {
    queue.value = queue.value.filter((q) => q.promptId !== promptId)
  }

  async function startAsync(
    onQuestion: () => void,
    onWrongPassphrase: () => void,
  ): Promise<void> {
    if (unlisten.length > 0) return
    unlisten = await Promise.all([
      listen<BridgePromptEvent>('browser-bridge-prompt', (event) => {
        queue.value = [...queue.value, event.payload]
        onQuestion()
      }),
      listen<{ promptId: string }>('browser-bridge-prompt-cancelled', (event) =>
        remove(event.payload.promptId),
      ),
      listen('browser-bridge-changed', () => {
        changes.value += 1
      }),
      listen('browser-bridge-wrong-passphrase', onWrongPassphrase),
    ])
  }

  async function answerAsync(
    promptId: string,
    answer: BridgeAnswer,
  ): Promise<void> {
    remove(promptId)
    await invoke('browser_bridge_answer', { args: { promptId, answer } })
  }

  return { queue, shown, changes, startAsync, answerAsync }
})
