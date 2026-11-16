import { invoke } from '@tauri-apps/api/core'
import { ref } from 'vue'
import type { AssistantName } from '~/types/bindings/AssistantName'
import { DEFAULT_ASSISTANT_NAME } from '~/lib/assistant/name'

/**
 * The name of the assistant in this vault (design 2026-11-16), shared by every view and by the
 * texts (`src/i18n/i18n.config.ts` reads it). Loaded when a vault is open and after every change
 * of the preferences (`src/plugins/assistantName.client.ts`).
 */
export const assistantName = ref<AssistantName>({ ...DEFAULT_ASSISTANT_NAME })

export async function loadAssistantNameAsync(): Promise<void> {
  assistantName.value = await invoke<AssistantName>('assistant_name_get')
}

export function useAssistantName() {
  async function setAsync(written: string, spoken: string): Promise<void> {
    assistantName.value = await invoke<AssistantName>('assistant_name_set', {
      written,
      spoken,
    })
  }

  async function resetAsync(): Promise<void> {
    assistantName.value = await invoke<AssistantName>('assistant_name_reset')
  }

  return { name: assistantName, setAsync, resetAsync }
}
