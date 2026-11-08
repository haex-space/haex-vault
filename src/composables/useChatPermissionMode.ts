import { ref, type Ref } from 'vue'
import type { usePreferences } from '~/composables/usePreferences'

const PERMISSION_MODE_KEY = 'chat.permission_mode'

/**
 * Permission-mode (manual/auto/plan) preference state (vault-wide since spec 023, FR-024) —
 * extracted from `src/pages/chat/[instance].vue` (spec 015-workspace-shell, T013) to bring the
 * orchestrator page under the 500-line constitution limit. The autonomy mode of an Abo-Agent is no
 * preference any more: it is a right of the agent profile of its model (spec 083 FR-030), which
 * Rust reads when it sends.
 */
export function useChatPermissionMode(
  getPrefAsync: ReturnType<typeof usePreferences>['getPrefAsync'],
  setPrefAsync: ReturnType<typeof usePreferences>['setPrefAsync'],
  errString: (e: unknown) => string,
  lastError: Ref<string | null>,
) {
  const permissionMode = ref<'manual' | 'auto' | 'plan'>('manual')
  const deviceUuid = ref('')
  const permissionModeSaving = ref(false)

  async function updatePermissionMode(mode: 'manual' | 'auto' | 'plan') {
    if (!deviceUuid.value || permissionModeSaving.value) return
    const previousMode = permissionMode.value
    permissionMode.value = mode
    permissionModeSaving.value = true
    try {
      await setPrefAsync({ kind: 'vault' }, PERMISSION_MODE_KEY, mode)
    } catch (e: unknown) {
      permissionMode.value = previousMode
      lastError.value = errString(e)
    } finally {
      permissionModeSaving.value = false
    }
  }

  /** Reads the persisted permission mode for `vaultDeviceUuid`, called once from
   * the page's `onMounted`. */
  async function initialize(vaultDeviceUuid: string) {
    const [permissionResult] = await Promise.allSettled([
      getPrefAsync({ kind: 'vault' }, PERMISSION_MODE_KEY),
    ])
    if (permissionResult.status === 'fulfilled') {
      const stored = permissionResult.value
      if (stored === 'manual' || stored === 'auto' || stored === 'plan') {
        permissionMode.value = stored
      }
    }
    deviceUuid.value = vaultDeviceUuid
  }

  // The mode is a vault preference another device or window can change (spec 024 FR-032).
  onVaultTablesChanged(['preferences'], async () => {
    if (!deviceUuid.value) return // not initialized yet
    if (!permissionModeSaving.value) {
      const stored = await getPrefAsync({ kind: 'vault' }, PERMISSION_MODE_KEY)
      if (stored === 'manual' || stored === 'auto' || stored === 'plan') {
        permissionMode.value = stored
      }
    }
  })

  return {
    permissionMode,
    deviceUuid,
    permissionModeSaving,
    updatePermissionMode,
    initialize,
  }
}
