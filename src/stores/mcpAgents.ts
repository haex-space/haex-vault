import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { defineStore } from 'pinia'
import type { PromptClosed } from '~/types/bindings/PromptClosed'
import type { RegistrationAnswer } from '~/types/bindings/RegistrationAnswer'
import type { RegistrationPromptEvent } from '~/types/bindings/RegistrationPromptEvent'

/**
 * The registration dialogs of external agents (spec 021, US1, contracts/tauri-commands.md): Rust
 * asks with `mcp-registration-request` and closes a dialog nobody waits for any more with
 * `mcp-prompt-closed`. One dialog is shown at a time, the oldest first. Only this window answers;
 * no agent can (FR-016).
 */
export const useMcpAgentsStore = defineStore('mcpAgents', () => {
  const queue = ref<RegistrationPromptEvent[]>([])
  const shown = computed(() => queue.value[0])
  /** Counts `mcp-agents-changed`, so the settings reload when it moves. */
  const changes = ref(0)
  let unlisten: UnlistenFn[] = []

  function remove(requestId: string): void {
    queue.value = queue.value.filter((q) => q.requestId !== requestId)
  }

  async function startAsync(onQuestion: () => void): Promise<void> {
    if (unlisten.length > 0) return
    unlisten = await Promise.all([
      listen<RegistrationPromptEvent>('mcp-registration-request', (event) => {
        queue.value = [...queue.value, event.payload]
        onQuestion()
      }),
      listen<PromptClosed>('mcp-prompt-closed', (event) =>
        remove(event.payload.requestId),
      ),
      listen('mcp-agents-changed', () => {
        changes.value += 1
      }),
    ])
  }

  async function answerAsync(
    requestId: string,
    answer: RegistrationAnswer,
  ): Promise<void> {
    remove(requestId)
    await invoke('mcp_registration_answer', { args: { requestId, answer } })
  }

  return { queue, shown, changes, startAsync, answerAsync }
})
