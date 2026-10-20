import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { AttachmentKind } from '~/composables/useChat'
import type {
  HuggingFaceInstallRequest,
  InstallPreview,
} from '~/composables/useHuggingFace'

/** `models.source_kind` — see storage/models.rs `SourceKind`. */
export type ModelSourceKind =
  'catalog' | 'huggingface' | 'imported' | 'provider'
/** `models.integrity_status` — see storage/models.rs `IntegrityStatus`. */
export type ModelIntegrityStatus = 'verified' | 'untrusted' | 'unknown'

/** One selectable reasoning option: a provider-native id plus a label. */
export interface ReasoningOption {
  id: string
  label: string
}

/**
 * A model's answer to "can the user influence reasoning?" (spec 012; mirrors
 * `model_capabilities.rs`). Absent (`null` on the capabilities) means not
 * determined — never the same as `unavailable`.
 */
export type ReasoningControl =
  | { kind: 'unavailable' }
  | { kind: 'model_managed' }
  | { kind: 'presets'; options: ReasoningOption[] }

/** What one provider/model pair supports; every field `null` = not determined. */
export interface ModelCapabilities {
  reasoning: ReasoningControl | null
  acceptedAttachmentKinds: AttachmentKind[] | null
  thinkingStyle: 'adaptive' | 'manual' | null
  /** Whether the model can call tools (spec 032); `null` = not determined. */
  toolUse: {
    support: 'supported' | 'unsupported'
    basis: 'provider' | 'curated' | 'template' | 'self_test'
  } | null
  /** What the user stated about a model of an OpenAI-compatible server (spec 055 FR-019), already
   * laid over the fields above; absent when nothing was stated. */
  user?: {
    acceptsImages: boolean | null
    effortLevels: string[] | null
  }
}

export interface InstalledModel {
  id: string
  name: string
  providerId: string
  contextWindow: number | null
  relativePath: string
  sizeBytes: number
  sourceKind: ModelSourceKind
  hfRepo: string | null
  hfFilename: string | null
  hfRevision: string | null
  hfRevisionRef: string | null
  fileSha256: string | null
  integrityStatus: ModelIntegrityStatus
  capabilities: ModelCapabilities | null
}

/** The three `HolziError` discriminants a pre-load integrity check can fail with. */
export type ModelIntegrityErrorKind =
  'ModelIntegrityMismatch' | 'ModelIntegrityUnknown' | 'ModelIntegrityError'

/** A failed pre-load integrity check, ready to drive `ModelIntegrityDialog`. */
export interface ModelIntegrityFailure {
  modelId: string
  errorKind: ModelIntegrityErrorKind
  expected: string | null
  actual: string | null
}

/**
 * Recognizes an integrity failure in a rejected `load_model` call and
 * narrows it to the dialog's state, or returns `null` when the rejection
 * was something else and the caller should surface its own error.
 *
 * `HolziError` is serialized with `#[serde(tag = "kind")]` only — no
 * `rename_all`, so the hash fields arrive snake_cased (HolziError.ts).
 */
export function parseModelIntegrityFailure(
  modelId: string,
  error: unknown,
): ModelIntegrityFailure | null {
  const err = error as {
    kind?: string
    expected_sha256?: string | null
    actual_sha256?: string | null
  }
  const kind = err.kind
  if (
    kind !== 'ModelIntegrityMismatch' &&
    kind !== 'ModelIntegrityUnknown' &&
    kind !== 'ModelIntegrityError'
  ) {
    return null
  }
  return {
    modelId,
    errorKind: kind,
    expected: err.expected_sha256 ?? null,
    actual: err.actual_sha256 ?? null,
  }
}

export interface DownloadProgressEvent {
  modelId: string
  bytesDownloaded: number
  bytesTotal: number | null
}

/** Emitted after a local model's tool-use capability was measured. */
export interface ModelToolUseUpdatedEvent {
  modelId: string
}

export interface DownloadFromHfArgs {
  id: string
  name: string
  hfRepo: string
  hfFilename: string
  hfRevision: string
  hfRevisionRef?: string | null
  tokenizerRepo: string
  contextWindow?: number | null
  forceTooBig?: boolean
  /** Bypass same-source idempotency for an explicit integrity repair. */
  forceRepair?: boolean
}

export interface ImportModelArgs {
  id: string
  name: string
  sourcePath: string
  tokenizerRepo: string
  contextWindow?: number | null
  filename?: string
}

/**
 * Shared shape behind every "list installed tiers of a built-in catalog /
 * download one" composable (spec 010): `useModels` (chat/agent models) and
 * `useSttModels` (speech-to-text models) both need exactly this pair,
 * against different Tauri commands and entry types — parameterized here so
 * neither file duplicates the implementation.
 */
export function makeInstalledModelsComposable<TModel>(
  listInstalledCommand: string,
  downloadFromCatalogCommand: string,
) {
  return function useGeneratedInstalledModels() {
    /** Lists models that are already fully installed on this device. */
    async function listInstalledAsync(): Promise<TModel[]> {
      return await invoke<TModel[]>(listInstalledCommand)
    }

    /** Downloads and registers one of the bundled catalog entries. */
    async function downloadFromCatalogAsync(
      catalogId: string,
    ): Promise<TModel> {
      return await invoke<TModel>(downloadFromCatalogCommand, { catalogId })
    }

    return { listInstalledAsync, downloadFromCatalogAsync }
  }
}

const useInstalledChatModels = makeInstalledModelsComposable<InstalledModel>(
  'list_installed_models',
  'download_model_from_catalog',
)

/**
 * Thin wrapper around the model management commands and the two
 * progress-related Tauri events (`model-download-progress`,
 * `model-download-complete`).
 */
export function useModels() {
  const { listInstalledAsync, downloadFromCatalogAsync } =
    useInstalledChatModels()

  /**
   * Installs a free HuggingFace GGUF: previews the install to resolve the
   * revision to a commit SHA and derive the local model id, then calls
   * the shared download command with that resolved contract
   * (contracts/tauri-commands.md §"Frontend-Composable-Vertrag"). Throws
   * `TokenizerRequired`-shaped errors from the backend if neither
   * `request.tokenizerRepo` nor the preview resolved one.
   */
  async function downloadFromHfAsync(
    request: HuggingFaceInstallRequest,
  ): Promise<InstalledModel> {
    const preview = await invoke<InstallPreview>(
      'preview_huggingface_install',
      {
        args: {
          repoId: request.repoId,
          filename: request.filename,
          revision: request.revision,
          tokenizerRepo: request.tokenizerRepo,
          contextWindow: request.contextWindow,
        },
      },
    )
    const tokenizerRepo = request.tokenizerRepo ?? preview.tokenizerRepo
    if (!tokenizerRepo) {
      throw new Error(
        'tokenizerRepo is required and could not be resolved automatically',
      )
    }
    const args: DownloadFromHfArgs = {
      id: preview.modelId,
      name: request.name,
      hfRepo: request.repoId,
      hfFilename: request.filename,
      hfRevision: preview.revision,
      hfRevisionRef: preview.revisionRef,
      tokenizerRepo,
      contextWindow: request.contextWindow ?? preview.contextWindow,
      forceTooBig: request.forceTooBig,
      forceRepair: request.forceRepair,
    }
    return await invoke<InstalledModel>('download_model_from_hf', { args })
  }

  /** Copies a local GGUF file into managed storage and registers it. */
  async function importFromFileAsync(
    args: ImportModelArgs,
  ): Promise<InstalledModel> {
    return await invoke<InstalledModel>('import_model_from_file', { args })
  }

  /** Deletes an installed model and its download record. */
  async function deleteAsync(id: string): Promise<void> {
    await invoke('delete_installed_model', { id })
  }

  /**
   * Subscribes to per-chunk progress events. Returns the unlisten
   * function — callers MUST call it on component unmount to avoid a
   * leaked listener.
   */
  async function onDownloadProgress(
    handler: (event: DownloadProgressEvent) => void,
  ): Promise<UnlistenFn> {
    return await listen<DownloadProgressEvent>(
      'model-download-progress',
      (e) => {
        handler(e.payload)
      },
    )
  }

  /** Subscribes to completed model downloads and returns the unlisten function. */
  async function onDownloadComplete(
    handler: (model: InstalledModel) => void,
  ): Promise<UnlistenFn> {
    return await listen<InstalledModel>('model-download-complete', (e) => {
      handler(e.payload)
    })
  }

  /** Subscribes to capability updates produced by a local-model probe/self-test. */
  async function onToolUseUpdated(
    handler: (event: ModelToolUseUpdatedEvent) => void,
  ): Promise<UnlistenFn> {
    return await listen<ModelToolUseUpdatedEvent>(
      'model-tool-use-updated',
      (e) => handler(e.payload),
    )
  }

  return {
    listInstalledAsync,
    downloadFromCatalogAsync,
    downloadFromHfAsync,
    importFromFileAsync,
    deleteAsync,
    onDownloadProgress,
    onDownloadComplete,
    onToolUseUpdated,
  }
}
