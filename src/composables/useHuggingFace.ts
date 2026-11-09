import { invoke } from '@tauri-apps/api/core'
import type {
  DownloadFromHfArgs,
  InstalledModel,
} from '~/composables/useModels'

/** Where a normalized quantization/context-window value came from. */
export type MetadataProvenanceSource =
  'hub_metadata' | 'filename_heuristic' | 'gguf_header' | 'unknown'

export interface MetadataProvenance {
  quantization: MetadataProvenanceSource
  contextWindow: MetadataProvenanceSource
}

/** Existing hardware-fit classifier verdict (see useCatalog.ts). */
export type HardwareFit = 'fits' | 'tight' | 'too_big' | 'unknown'

export interface HuggingFaceFileCandidate {
  repoId: string
  filename: string
  revision: string
  revisionRef: string | null
  sizeBytes: number | null
  quantization: string | null
  contextWindow: number | null
  tokenizerRepo: string | null
  tokenizerRequired: boolean
  fit: HardwareFit
  catalogMatch: boolean
  catalogEntryId: string | null
  metadataProvenance: MetadataProvenance
}

export interface HuggingFaceModelResult {
  repoId: string
  displayName: string
  author: string | null
  license: string | null
  downloads: number | null
  files: HuggingFaceFileCandidate[]
  sourceRevision: string | null
  revisionRef: string | null
}

/** One page of search hits; `nextCursor` continues the list, absent on the last page. */
export interface HuggingFaceSearchPage {
  results: HuggingFaceModelResult[]
  nextCursor: string | null
}

export interface PreviewInstallArgs {
  repoId: string
  filename: string
  revision?: string
  tokenizerRepo?: string
  contextWindow?: number | null
}

export interface InstallPreview {
  modelId: string
  name: string
  repoId: string
  filename: string
  revision: string
  revisionRef: string | null
  sizeBytes: number | null
  quantization: string | null
  contextWindow: number | null
  tokenizerRepo: string | null
  tokenizerRequired: boolean
  catalogMatch: boolean
  catalogEntryId: string | null
  metadataProvenance: MetadataProvenance
  fit: HardwareFit
  requiresExplicitTooBigConfirmation: boolean
}

/**
 * UI-facing installation intent (data-model.md `HuggingFaceInstallRequest`).
 * `useModels().downloadFromHfAsync` maps this into a `preview_huggingface_install`
 * call followed by `download_model_from_hf`'s `DownloadFromHfArgs` — the
 * caller never resolves a revision or derives a model id itself.
 */
export interface HuggingFaceInstallRequest {
  repoId: string
  filename: string
  revision?: string
  name: string
  tokenizerRepo?: string
  contextWindow?: number | null
  forceTooBig?: boolean
  /** Bypass same-source idempotency for an explicit integrity repair. */
  forceRepair?: boolean
}

export interface HuggingFaceUpdateStatus {
  modelId: string
  repoId: string
  revisionRef: string
  installedRevision: string
  latestRevision: string | null
  updateAvailable: boolean
  checkedAt: string
  errorCode: string | null
}

/**
 * Thin wrapper around the free Hugging Face discovery/install commands.
 * Stateless like every other composable in this project (`useModels`,
 * `useInstance`) — loading/error/retry state lives in the component that
 * calls it, e.g. `HuggingFaceSearch.vue`.
 */
export function useHuggingFace() {
  /**
   * Public repository search; an omitted query returns the default top-model view. Pass the
   * `nextCursor` of a page as `cursor` to get the page after it.
   */
  async function searchAsync(
    query?: string,
    cursor?: string,
  ): Promise<HuggingFaceSearchPage> {
    const args: { query?: string; cursor?: string } = {}
    if (query !== undefined) args.query = query
    if (cursor !== undefined) args.cursor = cursor
    return await invoke<HuggingFaceSearchPage>(
      'search_huggingface_models',
      args,
    )
  }

  /** Full file listing + metadata for one repository. */
  async function detailsAsync(
    repoId: string,
    revision?: string,
  ): Promise<HuggingFaceModelResult> {
    return await invoke<HuggingFaceModelResult>(
      'get_huggingface_model_details',
      { repoId, revision },
    )
  }

  /** Read-only preview: resolves the revision and classifies hardware fit. */
  async function previewInstallAsync(
    args: PreviewInstallArgs,
  ): Promise<InstallPreview> {
    return await invoke<InstallPreview>('preview_huggingface_install', { args })
  }

  /** Raw download call — callers normally go through `useModels().downloadFromHfAsync`. */
  async function downloadAsync(
    args: DownloadFromHfArgs,
  ): Promise<InstalledModel> {
    return await invoke<InstalledModel>('download_model_from_hf', { args })
  }

  /** Checks every installed HF model with a tracked ref for an upstream update. */
  async function checkUpdatesAsync(): Promise<HuggingFaceUpdateStatus[]> {
    return await invoke<HuggingFaceUpdateStatus[]>(
      'check_huggingface_model_updates',
    )
  }

  /** Installs the currently-checked update for an already-installed HF model. */
  async function installUpdateAsync(modelId: string): Promise<InstalledModel> {
    return await invoke<InstalledModel>('install_huggingface_update', {
      modelId,
    })
  }

  return {
    searchAsync,
    detailsAsync,
    previewInstallAsync,
    downloadAsync,
    checkUpdatesAsync,
    installUpdateAsync,
  }
}

/**
 * Maps a structured `HolziError`-shaped rejection from any HF command to
 * an i18n key under `errors.hf.*`. Backend errors carry no localized text
 * (`serde(tag = "kind")` only) — this is the one place that translates the
 * machine-readable `kind` into a UI-facing message key, mirroring the
 * `errors.${kind}` pattern already used for instance-management errors.
 */
/**
 * The last enriched search result list with the cursor of its next page, so going back to the
 * search view (spec 023) does not refetch every repository.
 * ponytail: one entry for the whole process and never invalidated; a changed repository shows up
 * after a new search. A per-query cache with an age limit would be the upgrade.
 */
interface RememberedSearch {
  query: string
  results: HuggingFaceModelResult[]
  nextCursor: string | null
}

let lastSearch: RememberedSearch | null = null

export function cachedSearchResults(query: string): RememberedSearch | null {
  return lastSearch?.query === query ? lastSearch : null
}

export function rememberSearchResults(
  query: string,
  results: HuggingFaceModelResult[],
  nextCursor: string | null,
) {
  lastSearch = { query, results, nextCursor }
}

export function forgetSearchResults() {
  lastSearch = null
}

export function hfErrorKey(e: unknown): string {
  const kind = structuredHfError(e)?.kind
  switch (kind) {
    case 'InvalidInput':
      return 'errors.hf.invalidInput'
    case 'Network':
      return 'errors.hf.network'
    case 'Timeout':
      return 'errors.hf.timeout'
    case 'HttpStatus':
      return 'errors.hf.httpStatus'
    case 'RateLimited':
      return 'errors.hf.rateLimited'
    case 'UnsupportedFormat':
      return 'errors.hf.unsupportedFormat'
    case 'TokenizerRequired':
      return 'errors.hf.tokenizerRequired'
    case 'HardwareConfirmationRequired':
      return 'errors.hf.hardwareConfirmationRequired'
    case 'ModelDownload':
      return 'errors.hf.modelDownload'
    case 'ModelRegistrationFailed':
      return 'errors.hf.modelRegistrationFailed'
    case 'ModelNotFound':
      return 'errors.hf.modelNotFound'
    default:
      return 'errors.hf.generic'
  }
}

/** Whether the person cancelled the download, which is no failure to show. */
export function isDownloadCancelled(e: unknown): boolean {
  return structuredHfError(e)?.kind === 'ModelDownloadCancelled'
}

/**
 * Returns the backend's technical reason when Tauri rejected with a
 * structured HolziError. Tauri can deliver that error either as an object or
 * as a JSON string, so both forms are accepted here.
 */
export function hfErrorDetail(e: unknown): string | null {
  const reason = structuredHfError(e)?.reason
  if (typeof reason === 'string' && reason.length > 0) return reason
  if (e instanceof Error && e.message.length > 0) return e.message
  if (typeof e === 'string' && e.length > 0) return e
  return null
}

function structuredHfError(
  e: unknown,
): { kind?: unknown; reason?: unknown } | null {
  if (e && typeof e === 'object' && 'kind' in e) {
    return e as { kind?: unknown; reason?: unknown }
  }
  if (typeof e !== 'string') return null
  try {
    const parsed: unknown = JSON.parse(e)
    if (parsed && typeof parsed === 'object' && 'kind' in parsed) {
      return parsed as { kind?: unknown; reason?: unknown }
    }
  } catch {
    // The native bridge may return a plain string; the caller still gets the
    // localized generic error in that case.
  }
  return null
}
