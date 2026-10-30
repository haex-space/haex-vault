import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { defineStore } from 'pinia'
import type { BrowserDecision } from '~/types/bindings/BrowserDecision'
import type { BrowserPromptClosed } from '~/types/bindings/BrowserPromptClosed'
import type { BrowserPromptEvent } from '~/types/bindings/BrowserPromptEvent'
import type { BrowserSessionView } from '~/types/bindings/BrowserSessionView'
import type { Remember } from '~/types/bindings/Remember'

/**
 * The questions and sessions of the agent browser (spec 065, contracts/tauri-commands.md): Rust
 * asks with `agent-browser-prompt` and closes a question nobody waits for any more with
 * `agent-browser-prompt-closed`. One question is shown at a time, the oldest first. The running
 * sessions are read again on `agent-browser-sessions-changed`; the sessions bar stops them. Only
 * this window answers and stops; no agent can.
 */
export const useAgentBrowserStore = defineStore('agentBrowser', () => {
  const prompts = ref<BrowserPromptEvent[]>([])
  const shown = computed(() => prompts.value[0])
  const sessions = ref<BrowserSessionView[]>([])
  let unlisten: UnlistenFn[] = []

  function remove(requestId: string): void {
    prompts.value = prompts.value.filter((q) => q.requestId !== requestId)
  }

  async function loadSessionsAsync(): Promise<void> {
    sessions.value = await invoke<BrowserSessionView[]>(
      'agent_browser_sessions',
    )
  }

  function reloadSessions(): void {
    loadSessionsAsync().catch((error: unknown) => {
      console.warn('[agent browser] reading the sessions failed', error)
    })
  }

  async function startAsync(onQuestion: () => void): Promise<void> {
    if (unlisten.length > 0) return
    unlisten = await Promise.all([
      listen('agent-browser-sessions-changed', reloadSessions),
      listen<BrowserPromptEvent>('agent-browser-prompt', (event) => {
        prompts.value = [...prompts.value, event.payload]
        onQuestion()
      }),
      listen<BrowserPromptClosed>('agent-browser-prompt-closed', (event) =>
        remove(event.payload.requestId),
      ),
    ])
    reloadSessions()
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

  /** "Stoppen" (FR-016): the running call ends, the tabs stay open for the user. */
  async function stopAsync(sessionId: string): Promise<void> {
    await invoke('agent_browser_control', {
      args: { sessionId, command: 'stop' },
    })
    sessions.value = sessions.value.filter((s) => s.id !== sessionId)
  }

  return {
    prompts,
    shown,
    sessions,
    startAsync,
    answerAsync,
    stopAsync,
  }
})
