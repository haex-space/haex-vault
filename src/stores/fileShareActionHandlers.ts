import { invoke } from '@tauri-apps/api/core'
import type { FileShare } from '@bindings/FileShare'
import type { FileShareGrantInput } from '@bindings/FileShareGrantInput'
import type { SourceRef } from '@bindings/SourceRef'
import type { useWindowManagerStore } from '~/stores/windowManager'

type WmStore = ReturnType<typeof useWindowManagerStore>

const text = (value: unknown) => String(value ?? '')

/**
 * The file share actions of the window (spec 056 US2, `lib/actions/fileShareActions.ts`): each
 * calls its Tauri command for the user; agents cannot call them yet (US6).
 */
export function registerFileShareActionHandlers(wm: WmStore): void {
  wm.registerGlobalActionHandler('files.shares.list', () =>
    invoke<FileShare[]>('file_shares_list'),
  )
  wm.registerGlobalActionHandler('files.shares.of', async ({ input }) => {
    const share = await invoke<FileShare | null>('file_share_of', {
      source: input.source as SourceRef,
      path: text(input.path),
    })
    return share ? { share } : {}
  })
  wm.registerGlobalActionHandler('files.shares.save', ({ input }) =>
    invoke<FileShare>('file_share_save', {
      args: {
        shareId: input.shareId === undefined ? undefined : text(input.shareId),
        source: input.source as SourceRef,
        path: text(input.path),
        name: text(input.name),
        grants: (input.grants ?? []) as FileShareGrantInput[],
      },
    }),
  )
  wm.registerGlobalActionHandler('files.shares.revoke', ({ input }) =>
    invoke<FileShare>('file_share_revoke', {
      shareId: text(input.shareId),
      vaultPubkey: text(input.vaultPubkey),
    }),
  )
  wm.registerGlobalActionHandler('files.shares.delete', async ({ input }) => {
    await invoke('file_share_delete', { shareId: text(input.shareId) })
    return {}
  })
  wm.registerGlobalActionHandler('files.shares.ofContact', ({ input }) =>
    invoke<FileShare[]>('file_shares_of_contact', {
      contactId: text(input.contactId),
    }),
  )
}
