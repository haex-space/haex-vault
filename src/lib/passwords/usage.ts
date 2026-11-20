// The warning before a delete (spec 034-password-manager, FR-034): which of the entries about to be
// deleted a haex-vault function uses. The functions report this themselves (`passwords_item_usage`); the
// password manager recognizes it by nothing else. Pure, so `scripts/check-passwords-usage.ts` runs
// it without vue.

export type ItemUsage = { itemId: string; features: readonly string[] }

export type UsageWarning = {
  /** The names of the functions, each once, sorted. */
  features: string[]
  /** The entries a function uses. */
  itemIds: string[]
}

/** The warning for a set of entries, or `null` when no function uses any of them. The delete is
 * allowed either way; the warning only names the functions. */
export function usageWarning(
  results: readonly ItemUsage[],
): UsageWarning | null {
  const used = results.filter((result) => result.features.length > 0)
  if (used.length === 0) return null
  const features = new Set<string>()
  for (const result of used)
    for (const name of result.features) features.add(name)
  return {
    features: [...features].sort((a, b) => a.localeCompare(b)),
    itemIds: used.map((result) => result.itemId),
  }
}
