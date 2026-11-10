import { invoke } from '@tauri-apps/api/core'

export type Fit = 'fits' | 'tight' | 'too_big' | 'unknown'

export interface CatalogEntry {
  id: string
  name: string
  family: string
  parameters: string
  quantization: string
  hf_repo: string
  hf_filename: string
  tokenizer_repo: string
  approx_size_bytes: number
  context_window: number
  license: string
  /** Only there to try: listed and downloadable, never suggested by onboarding. */
  trial?: boolean
}

export interface CatalogEntryWithFit extends CatalogEntry {
  fit: Fit
}

export type Tier = 'easy' | 'sweet' | 'max'

export interface TierRecommendation<TEntry = CatalogEntry> {
  tier: Tier
  entry: TEntry
  fit: Fit
}

/**
 * Shared shape behind every "browse a built-in model catalog" composable
 * (spec 010): list every entry annotated with a hardware-fit verdict, and
 * fetch exactly three tier recommendations for the onboarding wizard.
 * Parameterized by which Tauri commands to call and which entry type to
 * expect, so the chat catalog (`useCatalog`) and the STT catalog
 * (`useSttCatalog`) share one implementation instead of two near-identical
 * files.
 */
export function makeCatalogComposable<
  TEntry,
  TEntryWithFit extends { fit: Fit },
>(listCommand: string, recommendTiersCommand: string) {
  return function useGeneratedCatalog() {
    /** Returns every bundled catalog entry with its hardware-fit verdict. */
    async function listAsync(): Promise<TEntryWithFit[]> {
      return await invoke<TEntryWithFit[]>(listCommand)
    }

    /**
     * Returns exactly three tier-labelled recommendations
     * (Easy/Sweet/Max) for the onboarding wizard.
     */
    async function recommendTiersAsync(): Promise<
      TierRecommendation<TEntry>[]
    > {
      return await invoke<TierRecommendation<TEntry>[]>(recommendTiersCommand)
    }

    return { listAsync, recommendTiersAsync }
  }
}

/**
 * Reads the built-in chat/agent model catalog, annotated per entry with a
 * fit verdict against the current hardware. Serves the onboarding wizard's
 * "here are models that would work on this device" list.
 */
export const useCatalog = makeCatalogComposable<
  CatalogEntry,
  CatalogEntryWithFit
>('list_catalog', 'catalog_recommend_tiers')
