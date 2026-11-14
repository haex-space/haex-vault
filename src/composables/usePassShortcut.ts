import { invoke } from '@tauri-apps/api/core'

/**
 * The pinned shortcut of a pass on the home screen (spec 092, FR-038). Only Android can pin one;
 * elsewhere `supported` stays false and the action is not offered. The shortcut holds the opaque id
 * and the label the person typed, never the title and never a code. The launcher asks the person to
 * confirm, so nothing is said here when it worked.
 */
export function usePassShortcut() {
  const supported = ref(false)

  async function probeAsync(): Promise<void> {
    try {
      supported.value = await invoke<boolean>('pass_shortcut_supported')
    } catch {
      supported.value = false
    }
  }

  const pinAsync = (id: string, label: string) =>
    invoke('pass_shortcut_pin', { id, label })

  return { supported, probeAsync, pinAsync }
}
