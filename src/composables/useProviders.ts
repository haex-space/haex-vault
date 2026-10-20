import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { ModelCapabilities } from '~/composables/useModels'

export type ProviderKind = 'local' | 'api_key' | 'cli_delegate'

export type DelegateVendor = 'claude' | 'codex'

export interface Provider {
  id: string
  kind: ProviderKind
  adapter: string | null
  name: string
  baseUrl: string | null
  hasCredentials: boolean
  createdAt: number
}

export interface AddProviderArgs {
  kind: ProviderKind
  name: string
  adapter?: string
  baseUrl?: string
  apiKey?: string
}

/**
 * Result of an [`add_provider`] call. The provider is always inserted
 * on success; `refreshError` reports the auto-refresh outcome for
 * `api_key` providers — a non-null value means the row exists but
 * the model list could not be populated, so the frontend should toast
 * `refreshError` and let the operator retry [`refreshModelsAsync`].
 */
export interface AddProviderResult {
  provider: Provider
  modelCount: number | null
  refreshError: string | null
}

export interface ProviderModel {
  id: string
  name: string
  providerId: string
  contextWindow: number | null
  capabilities: ModelCapabilities | null
}

/** What the user states about a model of an OpenAI-compatible server (spec 055 FR-019); a field
 * left out is not stated, both left out withdraw the statement. */
export interface SetModelCapabilitiesArgs {
  modelId: string
  acceptsImages?: boolean
  effortLevels?: string[]
}

export interface RefreshProviderModelsResult {
  providerId: string
  modelCount: number
  fetchedAt: number
}

export interface ConnectCliDelegateArgs {
  vendor: DelegateVendor
  name: string
}

/**
 * Result of [`connectCliDelegateAsync`]. `codex` completes the whole flow
 * in one call and returns `'connected'`; `claude` needs a real pty and a
 * paste-back authorization code (research.md §5), so it returns
 * `'awaiting_code'` and the flow is completed by
 * [`submitCliDelegateCodeAsync`].
 */
export type ConnectCliDelegateResult =
  | { status: 'awaiting_code'; vendor: DelegateVendor }
  | { status: 'connected'; provider: Provider }

export interface SubmitCliDelegateCodeArgs {
  code: string
  name: string
}

/** Progress during `connectCliDelegateAsync`/`submitCliDelegateCodeAsync`.
 * `awaitingCode` is Claude-specific (contracts/tauri-commands.md); Codex
 * never emits it since its own process polls instead of taking a pasted
 * code back. */
export interface DelegateConnectProgressEvent {
  vendor: DelegateVendor
  status: 'awaiting_browser' | 'awaiting_code' | 'success' | 'error'
  url?: string
  code?: string
  message?: string
}

/**
 * Provider CRUD + refresh. `addAsync` auto-refreshes the model list
 * for `api_key` providers as part of the same round trip; explicit
 * [`refreshModelsAsync`] is available for re-fetching later.
 */
export function useProviders() {
  /** Lists every provider configured for the active instance. */
  async function listAsync(): Promise<Provider[]> {
    return await invoke<Provider[]>('list_providers')
  }

  /** Adds a provider to the active instance. */
  async function addAsync(args: AddProviderArgs): Promise<AddProviderResult> {
    return await invoke<AddProviderResult>('add_provider', { args })
  }

  /** Deletes a provider from the active instance. */
  async function deleteAsync(id: string): Promise<void> {
    await invoke('delete_provider', { id })
  }

  /** Re-fetches the given provider's model list and replaces its cache. */
  async function refreshModelsAsync(
    providerId: string,
  ): Promise<RefreshProviderModelsResult> {
    return await invoke<RefreshProviderModelsResult>(
      'refresh_provider_models',
      {
        providerId,
      },
    )
  }

  /** Reads the cached models for one provider. Does not trigger a fetch. */
  async function listModelsAsync(providerId: string): Promise<ProviderModel[]> {
    return await invoke<ProviderModel[]>('list_provider_models', { providerId })
  }

  /** Stores what the user states about a model; returns the record every reader now sees. */
  async function setModelCapabilitiesAsync(
    args: SetModelCapabilitiesArgs,
  ): Promise<ModelCapabilities> {
    return await invoke<ModelCapabilities>('set_model_user_capabilities', {
      args,
    })
  }

  /**
   * Starts a `cli_delegate` connect flow. For `codex` this resolves once
   * fully connected; for `claude` it resolves as soon as the OAuth URL is
   * known and must be followed by {@link submitCliDelegateCodeAsync}.
   */
  async function connectCliDelegateAsync(
    args: ConnectCliDelegateArgs,
  ): Promise<ConnectCliDelegateResult> {
    return await invoke<ConnectCliDelegateResult>('connect_cli_delegate', {
      args,
    })
  }

  /** Completes a Claude connect flow with the code copied from the browser. */
  async function submitCliDelegateCodeAsync(
    args: SubmitCliDelegateCodeArgs,
  ): Promise<Provider> {
    return await invoke<Provider>('submit_cli_delegate_code', { args })
  }

  /** Subscribes to connect-flow progress; call the returned unlisten on unmount. */
  async function onDelegateConnectProgress(
    handler: (event: DelegateConnectProgressEvent) => void,
  ): Promise<UnlistenFn> {
    return await listen<DelegateConnectProgressEvent>(
      'delegate-connect-progress',
      (event) => handler(event.payload),
    )
  }

  return {
    listAsync,
    addAsync,
    deleteAsync,
    refreshModelsAsync,
    listModelsAsync,
    setModelCapabilitiesAsync,
    connectCliDelegateAsync,
    submitCliDelegateCodeAsync,
    onDelegateConnectProgress,
  }
}
