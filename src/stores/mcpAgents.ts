import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { defineStore } from 'pinia'
import type { CallDecision } from '~/types/bindings/CallDecision'
import type { CallPromptEvent } from '~/types/bindings/CallPromptEvent'
import type { PromptClosed } from '~/types/bindings/PromptClosed'
import type { RegistrationAnswer } from '~/types/bindings/RegistrationAnswer'
import type { RegistrationPromptEvent } from '~/types/bindings/RegistrationPromptEvent'

/**
 * The questions of external agents (spec 021, US1, US3, contracts/tauri-commands.md): Rust asks with
 * `mcp-registration-request` and `mcp-call-request` and closes a question nobody waits for any
 * more with `mcp-prompt-closed`. One registration dialog and one call prompt are shown at a time,
 * the oldest first (FR-022). Only this window answers; no agent can (FR-016).
 */
export const useMcpAgentsStore = defineStore('mcpAgents', () => {
  const queue = ref<RegistrationPromptEvent[]>([])
  const shown = computed(() => queue.value[0])
  const calls = ref<CallPromptEvent[]>([])
  const shownCall = computed(() => calls.value[0])
  /** Counts `mcp-agents-changed`, so the settings reload when it moves. */
  const changes = ref(0)
  let unlisten: UnlistenFn[] = []

  function remove(requestId: string): void {
    queue.value = queue.value.filter((q) => q.requestId !== requestId)
    calls.value = calls.value.filter((q) => q.requestId !== requestId)
  }

  async function startAsync(
    onQuestion: (kind: 'registration' | 'call') => void,
  ): Promise<void> {
    if (unlisten.length > 0) return
    unlisten = await Promise.all([
      listen<RegistrationPromptEvent>('mcp-registration-request', (event) => {
        queue.value = [...queue.value, event.payload]
        onQuestion('registration')
      }),
      listen<CallPromptEvent>('mcp-call-request', (event) => {
        calls.value = [...calls.value, event.payload]
        onQuestion('call')
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
    await invoke('mcp_registration_answer', { args: { requestId, answer } })
    remove(requestId)
  }

  async function answerCallAsync(
    requestId: string,
    decision: CallDecision,
  ): Promise<void> {
    await invoke('mcp_call_answer', { args: { requestId, decision } })
    remove(requestId)
  }

  return {
    queue,
    shown,
    calls,
    shownCall,
    changes,
    startAsync,
    answerAsync,
    answerCallAsync,
  }
})
