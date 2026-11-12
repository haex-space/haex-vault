// Helper for the Wallet scenarios (spec 092, Phase W): opening the app. Passes are made through
// `createPass` of `passwords.ts` (the same vault data and service as the password manager).
import { resizeAppWindow } from './settings.ts'
import type { FlowInstance } from './flows.ts'

/** Opens Wallet from the launcher, fits its window into the virtual screen and waits for its
 * toolbar. */
export async function openWallet(instance: FlowInstance): Promise<void> {
  await instance.click('open-launcher')
  await instance.click('[data-app-id="system.wallet"]')
  await instance.waitForDisplayed('wallet-search')
  await resizeAppWindow(instance, 'system.wallet', 760, 560)
  await instance.waitForDisplayed('wallet-new')
}
