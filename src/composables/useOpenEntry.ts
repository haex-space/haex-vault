import type { TypedHeader } from '~/lib/passwords/passType'
import { entryTarget } from '~/lib/passwords/scope'

/**
 * Opens an entry where it lives (spec 092, Phase W): in this tab when the entry belongs to the app
 * the tab runs, else in the other app (its open window, or a new one). "Linked" and "Belongs to"
 * use it, so a login lists its passes and opens each in Wallet, and a pass opens the login it
 * belongs to in the password manager.
 */
export function useOpenEntry() {
  const router = useTabRouter()
  const { appId, openApp } = useWmTab()

  return function openEntry(header: (TypedHeader & { id: string }) | null) {
    const target = entryTarget(header)
    if (!target) return
    if (target.appId === appId) void router.push(target.path)
    else openApp(target.appId, target.path)
  }
}
