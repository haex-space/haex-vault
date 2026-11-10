import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { LinkCodeInfo } from '@bindings/LinkCodeInfo'
import type { LinkJoinState } from '@bindings/LinkJoinState'
import type { LinkJoinStartArgs } from '@bindings/LinkJoinStartArgs'
import type { LinkingStatus } from '@bindings/LinkingStatus'
import type { LinkText } from '@bindings/LinkText'

const LINK_HOST_STATE_CHANGED = 'link-host-state-changed'
const LINK_JOIN_STATE_CHANGED = 'link-join-state-changed'

/**
 * The commands and events of linking a device (spec 024, user story 5,
 * contracts/tauri-commands.md). Two sides:
 *
 * - the host, a main device with an open vault, shows a code
 *   (`createCodeAsync`) and answers the question about the new device
 *   (`confirmAsync`, `rejectAsync`);
 * - the joining installation, on the landing page with no vault open,
 *   enters the code (`joinStartAsync`) and follows `LinkJoinState`.
 *
 * The state of each side comes as an event; `onHostState` and `onJoinState`
 * return the function that stops listening.
 */
export function useDeviceLink() {
  async function createCodeAsync(): Promise<LinkCodeInfo> {
    return await invoke<LinkCodeInfo>('link_code_create')
  }

  async function cancelCodeAsync(): Promise<void> {
    await invoke('link_code_cancel')
  }

  async function confirmAsync(asMainDevice: boolean): Promise<void> {
    await invoke('link_confirm', { args: { asMainDevice } })
  }

  async function rejectAsync(): Promise<void> {
    await invoke('link_reject')
  }

  async function joinStartAsync(
    args: LinkJoinStartArgs,
  ): Promise<LinkJoinState> {
    return await invoke<LinkJoinState>('link_join_start', { args })
  }

  /** Reads a link text or a plain code (spec 085); works with no vault open. */
  async function parseTextAsync(text: string): Promise<LinkText> {
    return await invoke<LinkText>('link_text_parse', { args: { text } })
  }

  async function joinCancelAsync(): Promise<void> {
    await invoke('link_join_cancel')
  }

  function onHostState(
    handler: (status: LinkingStatus | null) => void,
  ): Promise<UnlistenFn> {
    return listen<LinkingStatus | null>(LINK_HOST_STATE_CHANGED, (event) =>
      handler(event.payload),
    )
  }

  function onJoinState(
    handler: (state: LinkJoinState) => void,
  ): Promise<UnlistenFn> {
    return listen<LinkJoinState>(LINK_JOIN_STATE_CHANGED, (event) =>
      handler(event.payload),
    )
  }

  return {
    createCodeAsync,
    cancelCodeAsync,
    confirmAsync,
    rejectAsync,
    joinStartAsync,
    joinCancelAsync,
    parseTextAsync,
    onHostState,
    onJoinState,
  }
}
