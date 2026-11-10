import { invoke } from '@tauri-apps/api/core'
import type { FilesAgentShow } from '@bindings/FilesAgentShow'
import { ActionInputError } from '~/lib/actions/runner'
import { FILES_APP_ID, filesLocation } from '~/lib/files/registry'
import type { useWindowManagerStore } from '~/stores/windowManager'

type WmStore = ReturnType<typeof useWindowManagerStore>

/**
 * The one file action of the window (spec 044 FR-032a, `lib/actions/filesActions.ts`):
 * `files.show` opens the file browser. Rust checks first what the agent may reach
 * (`files_agent_check`, the checks of `files.stat`) and says where to open; the other file actions
 * run in Rust without a window (ADR 0011).
 */
export function registerFilesActionHandlers(wm: WmStore): void {
  wm.registerGlobalActionHandler('files.show', async ({ input, caller }) => {
    const show = await invoke<FilesAgentShow>('files_agent_check', {
      source: String(input.source ?? ''),
      path: String(input.path ?? ''),
      agentCall: caller.kind !== 'user',
      modelId: caller.kind === 'builtinAgent' ? (caller.modelId ?? null) : null,
      localModel:
        caller.kind === 'builtinAgent' ? (caller.localModel ?? false) : false,
      externalAgentId: caller.kind === 'externalAgent' ? caller.agentId : null,
    })
    if (!show.allowed || !show.source || show.folder === null) {
      throw new ActionInputError(show.reason ?? 'not available', 'path')
    }
    const tabId = wm.openApp(
      FILES_APP_ID,
      filesLocation({
        source: show.source,
        path: show.folder,
        ...(show.open ? { open: show.open } : {}),
      }),
    )
    return { opened: tabId !== null }
  })
}
