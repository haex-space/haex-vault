import { invoke } from '@tauri-apps/api/core'
import type { AgentMemoryPage } from '@bindings/AgentMemoryPage'
import type { useWindowManagerStore } from '~/stores/windowManager'

type WmStore = ReturnType<typeof useWindowManagerStore>

const text = (value: unknown) => String(value ?? '')

/**
 * The actions of "KI → Gedächtnis" (spec 097, `lib/actions/settingsAgentMemoryActions.ts`): the
 * user adds, changes and removes pages of the agents' shared memory. Guardrails, so no agent calls
 * them; each calls its Tauri command, which refuses with the code of the contract.
 */
export function registerAgentMemoryActionHandlers(wm: WmStore): void {
  wm.registerGlobalActionHandler('settings.agentMemory.add', ({ input }) =>
    invoke<AgentMemoryPage>('agent_memory_add', {
      args: {
        title: text(input.title),
        description: text(input.description),
        body: text(input.body),
      },
    }),
  )
  wm.registerGlobalActionHandler('settings.agentMemory.change', ({ input }) =>
    invoke<AgentMemoryPage>('agent_memory_change', {
      args: {
        id: text(input.id),
        title: text(input.title),
        description: text(input.description),
        body: text(input.body),
      },
    }),
  )
  wm.registerGlobalActionHandler('settings.agentMemory.remove', ({ input }) =>
    invoke('agent_memory_remove', { args: { id: text(input.id) } }),
  )
}
