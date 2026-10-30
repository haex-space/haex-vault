import type { Fit } from '~/composables/useCatalog'
import { makeCatalogComposable } from '~/composables/useCatalog'

/** Which local engine runs a model (spec 066, research R7). */
export type SttEngine = 'candle-whisper' | 'sherpa-parakeet'

export interface SttCatalogEntry {
  id: string
  name: string
  hfRepo: string
  hfRevision: string
  approxSizeBytes: number
  license: string
  engine: SttEngine
  /** ISO 639-1 codes; empty for Whisper's open-ended list. */
  languages: string[]
  /** Credit the licence asks for (CC BY). */
  attribution?: string | null
}

export interface SttCatalogEntryWithFit extends SttCatalogEntry {
  fit: Fit
}

/**
 * Reads the built-in local speech-to-text model catalog (spec 010),
 * annotated per entry with a fit verdict against the current hardware.
 * Shares its implementation with `useCatalog` (the chat/agent model
 * catalog) via `makeCatalogComposable` — only the Tauri command names and
 * entry types differ.
 */
export const useSttCatalog = makeCatalogComposable<
  SttCatalogEntry,
  SttCatalogEntryWithFit
>('list_stt_catalog', 'stt_recommend_tiers')
