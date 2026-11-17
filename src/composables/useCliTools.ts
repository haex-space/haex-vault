import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { toast } from 'vue-sonner'
import { isCancelledInstall } from '~/lib/cli-tools'
import type { CliToolChanged } from '~/types/bindings/CliToolChanged'
import type { CliToolProgress } from '~/types/bindings/CliToolProgress'
import type { CliToolStatus } from '~/types/bindings/CliToolStatus'

/**
 * The CLI tools of the open vault (spec 094): which are installed, and the install and its
 * progress. The state on disk is the truth; a window reads it again whenever a tool changed, so a
 * second window of the same vault shows the same row. The install is also the update (the backend
 * installs the newest listed one next to the one in use), and a revert goes back from an update
 * that has not run yet. A failed action is a toast, with a fixed id
 * so that repeating it replaces the toast instead of piling up; the new state is the feedback of
 * a success, there is no success line.
 */
export function useCliTools() {
  const { errString } = useErrorString()

  const statuses = ref<CliToolStatus[]>([])
  const loaded = ref(false)
  /** The latest progress of a running install, by vendor. */
  const progress = reactive<Record<string, CliToolProgress | undefined>>({})
  const unlisteners: UnlistenFn[] = []
  let started = false
  let lifecycle = 0

  function statusOf(vendor: string): CliToolStatus | undefined {
    return statuses.value.find((status) => status.vendor === vendor)
  }

  async function refreshAsync() {
    try {
      statuses.value = await invoke<CliToolStatus[]>('cli_tools_status')
    } catch (error) {
      toast.error(errString(error), { id: 'cli-tools-status' })
    } finally {
      loaded.value = true
    }
  }

  async function installAsync(vendor: string) {
    progress[vendor] = undefined
    try {
      await invoke<CliToolStatus>('cli_tools_install', { vendor })
    } catch (error) {
      // A cancel by the user is not a failure to report.
      if (!isCancelledInstall(error)) {
        toast.error(errString(error), { id: 'cli-tool-progress' })
      }
    } finally {
      progress[vendor] = undefined
      await refreshAsync()
    }
  }

  async function revertAsync(vendor: string) {
    try {
      await invoke<CliToolStatus>('cli_tools_revert', { vendor })
    } catch (error) {
      toast.error(errString(error), { id: `cli-tool-revert-${vendor}` })
    } finally {
      await refreshAsync()
    }
  }

  async function cancelAsync(vendor: string) {
    try {
      await invoke('cli_tools_cancel', { vendor })
    } catch (error) {
      toast.error(errString(error), { id: `cli-tools-cancel-${vendor}` })
    }
  }

  /** Reads the tools and follows the events of the backend; call `stop` when the view ends. */
  async function startAsync() {
    if (started) return
    started = true
    const currentLifecycle = ++lifecycle
    const subscriptions = [
      listen<CliToolProgress>('cli-tool-progress', (event) => {
        progress[event.payload.vendor] = event.payload
      }),
      listen<CliToolChanged>('cli-tool-changed', () => {
        void refreshAsync()
      }),
    ]
    await Promise.all(
      subscriptions.map(async (subscription) => {
        const unlisten = await subscription
        if (!started || currentLifecycle !== lifecycle) unlisten()
        else unlisteners.push(unlisten)
      }),
    )
    if (started && currentLifecycle === lifecycle) await refreshAsync()
  }

  function stop() {
    if (!started) return
    started = false
    lifecycle += 1
    for (const unlisten of unlisteners.splice(0)) unlisten()
  }

  return {
    statuses,
    loaded,
    progress,
    statusOf,
    refreshAsync,
    installAsync,
    revertAsync,
    cancelAsync,
    startAsync,
    stop,
  }
}
