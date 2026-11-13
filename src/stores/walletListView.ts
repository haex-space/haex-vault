import { ref } from 'vue'
import { defineStore } from 'pinia'

export type WalletSortMode = 'date' | 'title'

/**
 * How Wallet's list is ordered and trimmed (spec 092, FR-010, FR-021, Phase W): by date (upcoming
 * first, then over, then undated) or by title, and whether passes that are over show. It is kept
 * while the vault is open and shared by the windows of Wallet; it is a view setting, not a place,
 * so it is not in the route. The filter by kind is: it is a place (`?kind=`) and survives back and
 * forward.
 */
export const useWalletListViewStore = defineStore('walletListView', () => {
  const sort = ref<WalletSortMode>('date')
  const hideExpired = ref(false)

  function reset() {
    sort.value = 'date'
    hideExpired.value = false
  }

  return { sort, hideExpired, reset }
})
