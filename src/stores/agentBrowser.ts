import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { defineStore } from 'pinia'
import type { BrowserDecision } from '~/types/bindings/BrowserDecision'
import type { BrowserPromptClosed } from '~/types/bindings/BrowserPromptClosed'
import type { BrowserPromptEvent } from '~/types/bindings/BrowserPromptEvent'
import type { Remember } from '~/types/bindings/Remember'

/**
 * The questions of the agent browser (spec 065, contracts/tauri-commands.md): Rust asks with
 * `agent-browser-prompt` and closes a question nobody waits for any more with
 * `agent-browser-prompt-closed`. One question is shown at a time, the oldest first. Only this
 * window answers; no agent can.
 */
export const useAgentBrowserStore = defineStore('agentBrowser', () => {
  const prompts = ref<BrowserPromptEvent[]>([])
  const shown = computed(() => prompts.value[0])
  let unlisten: UnlistenFn[] = []

  function remove(requestId: string): void {
    prompts.value = prompts.value.filter((q) => q.requestId !== requestId)
  }

  async function startAsync(onQuestion: () => void): Promise<void> {
    if (unlisten.length > 0) return
    unlisten = await Promise.all([
      listen<BrowserPromptEvent>('agent-browser-prompt', (event) => {
        prompts.value = [...prompts.value, event.payload]
        onQuestion()
      }),
      listen<BrowserPromptClosed>('agent-browser-prompt-closed', (event) =>
        remove(event.payload.requestId),
      ),
    ])
  }

  async function answerAsync(
    requestId: string,
    decision: BrowserDecision,
    remember?: Remember,
  ): Promise<void> {
    await invoke('agent_browser_prompt_answer', {
      args: { requestId, decision, remember },
    })
    remove(requestId)
  }

  return { prompts, shown, startAsync, answerAsync }
})
