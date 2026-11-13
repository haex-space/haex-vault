import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import type { UnlistenFn } from '@tauri-apps/api/event'
import type {
  LoadedModelInfo,
  ModelLoadErrorEvent,
  ModelLoadPhase,
  ModelLoadProgressEvent,
  ModelLoadStatusPayload,
} from '~/composables/useChat'
import type { CatalogEntryWithFit } from '~/composables/useCatalog'
import type { ResolveDefaultModelResult } from '~/composables/usePreferences'
import { useModelInventory } from '~/composables/useModelInventory'
import { useModelIntegrity } from '~/composables/useModelIntegrity'
import { useReasoningPreference } from '~/composables/useReasoningPreference'

/**
 * Model lifecycle: install/catalog/provider listing, load/unload, download
 * progress, and the pre-load integrity dialog. Exactly one model is ever
 * active at a time, so this is a plain singleton store rather than
 * something keyed by id. The lists/picker grouping and the integrity dialog
 * live in `useModelInventory`/`useModelIntegrity` (spaex 500-LoC boundary);
 * this store owns the load lifecycle and re-exports their state unchanged.
 *
 * `busy` and `lastError` for the composer/send flow stay on the chat page —
 * they're shared with logic that has nothing to do with models, and Pinia
 * setup stores can't take page-local refs as constructor params. `loadModel`
 * here does not touch the page's `busy`; callers that need to block on it
 * (`newChat`) check `modelLoadPending` directly instead.
 */
export const useModelsStore = defineStore('models', () => {
  const chat = useChat()
  const models = useModels()
  const catalog = useCatalog()
  const providers = useProviders()
  const {
    resolveDefaultModelAsync,
    getPrefAsync,
    setPrefAsync,
    clearPrefAsync,
  } = usePreferences()
  const { currentDeviceInfoAsync } = useDevice()
  const { t } = useI18n()
  const { errString } = useErrorString()

  const activeModel = ref<LoadedModelInfo | null>(null)
  // The local model held in memory, apart from the chat session's model above: it stays loaded
  // until the person unloads it in the settings, whatever the chat chooses.
  const activeLocalModel = ref<LoadedModelInfo | null>(null)
  const activeLocalModelId = computed(
    () => activeLocalModel.value?.modelId ?? null,
  )
  const modelLoadPending = ref(false)
  const lastError = ref<string | null>(null)
  const setError = (message: string) => {
    lastError.value = message
  }
  // This vault device's uuid is used as the readiness gate for the
  // vault-scoped per-model reasoning preference. Resolved once at the start of
  // `initialize()`; nothing is read or written before it is known.
  const vaultDeviceUuid = ref<string | null>(null)

  const { capabilities: deviceCapabilities } = useDeviceCapabilities()
  const {
    installedModels,
    catalogEntries,
    providerList,
    providerModels,
    noModelsInstalled,
    modelGroups,
    findModelName,
    capabilitiesFor,
    refreshInstalledAndCatalog,
    refreshProviders,
  } = useModelInventory({
    models,
    catalog,
    providers,
    t,
    errString,
    setError,
    cliDelegates: () => deviceCapabilities.value?.cliDelegates ?? true,
    activeLocalModelId: () => activeLocalModelId.value,
  })

  const downloadingId = ref<string | null>(null)
  const downloadProgressBytes = ref(0)
  const downloadTotalBytes = ref<number | null>(null)

  // Structured loading state driven by the `model-load-progress` event. The
  // composer stays available for drafting while sending remains blocked
  // until the load reaches `ready`.
  const loadingPhase = ref<ModelLoadPhase | null>(null)
  const loadingModelName = ref('')
  const loadingProviderName = ref<string | null>(null)
  // Which model `loadModel()` is currently targeting — set synchronously,
  // before the `load_model` round trip even starts, purely from
  // `modelGroups` (already fetched). Backs `displayModelId`/
  // `displayModelName` below so the composer reflects a fresh pick
  // immediately instead of only once the load resolves or the backend's
  // first `model-load-progress` event arrives.
  const loadingModelId = ref<string | null>(null)
  const loadErrorModelId = ref<string | null>(null)
  let loadingModelToken = 0

  // Read through a computed rather than `activeModel?.modelId` directly in
  // the template — vue-tsc narrows `activeModel` to `never` at the model
  // picker's `v-else-if="!activeModel"` (a chained-`v-if` control-flow
  // quirk), which a plain computed's independent return type sidesteps.
  const activeModelId = computed(() => activeModel.value?.modelId ?? '')

  // What the composer's model control should show: the model a load is
  // currently targeting, if any, else the actually-active one. Without
  // this, picking a new model left the composer showing the previous one
  // (or nothing) until `load_model`'s round trip resolved — `activeModel`
  // itself stays untouched until then, on purpose (`sendDisabled` etc.
  // must keep treating a pending load as "not ready"). `loadingModelId` is
  // set/cleared in lockstep by every load path — `loadModel`,
  // `onIntegrityLoadUntrusted`, and the backend-driven
  // `applyLoadProgress`/`applyLoadStatus`/`applyLoadError` handlers below
  // (an activation from the settings never calls `loadModel`) — so checking
  // it alone is sufficient; the extra `modelLoadPending.value &&` this used
  // to require was redundant for the frontend-initiated path and actively
  // wrong for the backend-initiated one.
  const displayModelId = computed(() =>
    loadingModelId.value ? loadingModelId.value : activeModelId.value,
  )
  const displayModelName = computed(() =>
    loadingModelId.value
      ? loadingModelName.value
      : (activeModel.value?.name ?? undefined),
  )

  // The displayed model's cached capabilities, read from the lists above —
  // no IPC and no second cache. `undefined` = no resolved row.
  const displayModelRecord = computed(() =>
    capabilitiesFor(displayModelId.value),
  )
  const displayModelCapabilities = computed(
    () => displayModelRecord.value ?? null,
  )
  const { effortLevel, effortOptions, effortState, updateEffortLevel } =
    useReasoningPreference({
      modelId: displayModelId,
      capabilities: displayModelRecord,
      deviceUuid: vaultDeviceUuid,
      preferences: { getPrefAsync, setPrefAsync, clearPrefAsync },
      errString,
      setError,
    })

  /** Localised label for the current loading phase, if any. */
  const loadingLabel = computed<string | null>(() => {
    const phase = loadingPhase.value
    if (!phase || phase === 'ready') return null
    const key =
      phase === 'connecting'
        ? 'chat.loading.connecting'
        : phase === 'cuda-jit-warmup'
          ? 'chat.loading.cudaJitWarmup'
          : 'chat.loading.loading'
    return t(key, {
      modelName: loadingModelName.value,
      providerName: loadingProviderName.value ?? '',
    })
  })

  /** Refreshes the backend's currently active model snapshot. */
  async function refreshActiveModel() {
    activeModel.value = await chat.activeModelInfoAsync()
  }

  /** Refreshes the snapshot of the active local model. */
  async function refreshActiveLocalModel() {
    activeLocalModel.value = await chat.activeLocalModelInfoAsync()
  }

  function beginLoadingModel(id: string, name: string): number {
    loadingModelToken += 1
    loadingModelId.value = id
    loadingModelName.value = name
    return loadingModelToken
  }

  function clearLoadingModel(token?: number) {
    if (token !== undefined && token !== loadingModelToken) return
    loadingModelToken += 1
    loadingModelId.value = null
    loadingModelName.value = ''
  }

  async function finishLoadingModel(token: number) {
    try {
      await Promise.all([refreshActiveModel(), refreshActiveLocalModel()])
      clearLoadingModel(token)
    } catch (e: unknown) {
      lastError.value = errString(e)
      return
    }
    // A load ends here that did not come from a call of ours (an activation in the settings):
    // when nothing is the chat's model yet, the last pick is again.
    await restoreLastModel()
  }

  const {
    integrityDialog,
    integrityBusy,
    integrityActionError,
    openIntegrityDialog,
    onIntegrityLoadUntrusted,
    onIntegrityRepairSource,
    onIntegrityChooseOther,
    onIntegrityDialogOpenChange,
  } = useModelIntegrity({
    chat,
    models,
    t,
    errString,
    installedModels,
    activeLocalModel,
    modelLoadPending,
    findModelName,
    beginLoadingModel,
    clearLoadingModel,
    refreshInstalledAndCatalog,
    setError,
  })

  /**
   * Loads the selected model (local or api_key composite id). `pick`: the person chose it in the
   * model menu, so a new session starts with it (spec 002 FR-009 as amended 2026-11-02);
   * `threadId`: the session open while picking, which answers with it from now on.
   */
  async function loadModel(
    id: string,
    options: { pick?: boolean; threadId?: string | null } = {},
  ) {
    lastError.value = null
    loadErrorModelId.value = null
    modelLoadPending.value = true
    const loadToken = beginLoadingModel(id, findModelName(id))
    try {
      activeModel.value = options.pick
        ? await chat.selectModelAsync(id, options.threadId ?? null)
        : await chat.loadModelAsync(id)
    } catch (e: unknown) {
      if (!openIntegrityDialog(id, e)) {
        lastError.value = errString(e)
      }
      loadingPhase.value = null
      // The backend never touches its existing session unless this load
      // fully succeeds (`load_model_inner` only swaps `chat.session` at
      // the very end) — so unconditionally nulling `activeModel` here is
      // wrong whenever this call was switching away from an already-good
      // model, including when the failure is the backend rejecting this
      // request in favor of a *different* load that's genuinely in
      // flight (the operation mutex, or an activation's own
      // generation counter racing this call — see
      // `restoreLastModel`). Re-read backend truth instead of
      // guessing "nothing is active".
      await refreshActiveModel().catch(() => {})
    } finally {
      modelLoadPending.value = false
      clearLoadingModel(loadToken)
    }
    // A chat load of the active local model puts it in memory on the way, and an activation may
    // have finished meanwhile; its `Ready` is not sent while this load holds the status.
    await refreshActiveLocalModel().catch(() => {})
  }

  /**
   * Loads the installed local model into memory and makes it the active one; the one that was
   * active is unloaded. Apart from the chat session: it stays loaded until it is unloaded here,
   * whatever the chat chooses. A failed integrity check opens the integrity dialog.
   */
  async function activateLocalModel(id: string) {
    lastError.value = null
    loadErrorModelId.value = null
    modelLoadPending.value = true
    const loadToken = beginLoadingModel(id, findModelName(id))
    try {
      activeLocalModel.value = await chat.activateLocalModelAsync(id)
    } catch (e: unknown) {
      if (!openIntegrityDialog(id, e)) {
        lastError.value = errString(e)
      }
      loadingPhase.value = null
      // The backend keeps the model that was active unless this activation fully succeeds.
      await refreshActiveLocalModel().catch(() => {})
    } finally {
      modelLoadPending.value = false
      clearLoadingModel(loadToken)
    }
    // A chat session that ran the old local model ended with it.
    await refreshActiveModel().catch(() => {})
  }

  /** Unloads the active local model; a chat session that ran it ends with it. */
  async function deactivateLocalModel() {
    lastError.value = null
    try {
      await chat.deactivateLocalModelAsync()
      activeLocalModel.value = null
    } catch (e: unknown) {
      lastError.value = errString(e)
    }
    await refreshActiveModel().catch(() => {})
  }

  /**
   * Downloads a catalog model, refreshes the lists, activates it and makes it the chat's model:
   * whoever downloads from the chat wants to chat with it.
   */
  async function downloadCatalogEntry(entry: CatalogEntryWithFit) {
    lastError.value = null
    downloadingId.value = entry.id
    downloadProgressBytes.value = 0
    downloadTotalBytes.value = entry.approx_size_bytes
    try {
      await models.downloadFromCatalogAsync(entry.id)
      await refreshInstalledAndCatalog()
      await activateLocalModel(entry.id)
      if (activeLocalModelId.value === entry.id) {
        await loadModel(entry.id, { pick: true })
      }
    } catch (e: unknown) {
      lastError.value = errString(e)
    } finally {
      downloadingId.value = null
    }
  }

  /** Applies an incremental model-load progress event to store state. */
  function applyLoadProgress(e: ModelLoadProgressEvent) {
    loadErrorModelId.value = null
    loadingPhase.value = e.phase
    loadingModelName.value = e.modelName
    // Backend-driven loads — chiefly an activation from the settings,
    // which never goes through `loadModel()` at all — would otherwise
    // leave `loadingModelId` unset, so `displayModelId`/`displayModelName`
    // fall back to the (still empty) active model and the composer shows
    // "Choose a model" the whole time a load that the loading banner
    // already reports is running.
    const loadToken = beginLoadingModel(e.modelId, e.modelName)
    loadingProviderName.value = e.providerName ?? null
    if (e.phase === 'ready') {
      loadingPhase.value = null
      void finishLoadingModel(loadToken)
    }
  }

  /** Reconciles store state with a complete model-load status snapshot. */
  function applyLoadStatus(status: ModelLoadStatusPayload) {
    if (status.status === 'loading') {
      loadErrorModelId.value = null
      loadingPhase.value = status.phase
      loadingModelName.value = status.modelName
      beginLoadingModel(status.modelId, status.modelName)
      loadingProviderName.value = status.providerName ?? null
    } else {
      loadingPhase.value = null
      loadingModelName.value =
        'modelName' in status ? (status.modelName ?? '') : ''
      loadingProviderName.value = null
      loadErrorModelId.value =
        status.status === 'error' ? (status.modelId ?? null) : null
      if (status.status === 'error') {
        clearLoadingModel()
        lastError.value = t('chat.loading.error')
      }
      if (status.status === 'ready') {
        const loadToken = beginLoadingModel(status.modelId, status.modelName)
        void finishLoadingModel(loadToken)
      }
      if (status.status === 'idle') clearLoadingModel()
    }
  }

  /** Records a terminal model-load error event. */
  function applyLoadError(event: ModelLoadErrorEvent) {
    loadingPhase.value = null
    clearLoadingModel()
    loadErrorModelId.value = event.modelId ?? null
    lastError.value = t('chat.loading.error')
  }

  /** Retries the model most recently associated with a load error. */
  async function retryModelLoad() {
    const modelId = loadErrorModelId.value
    if (!modelId) return
    await loadModel(modelId)
  }

  /** Fetches the model-load snapshot and every installed/catalog/provider list. */
  async function initialize() {
    lastError.value = null
    loadErrorModelId.value = null
    // First, so the displayed model's saved reasoning option can load as soon
    // as the active model is known below.
    try {
      vaultDeviceUuid.value = (await currentDeviceInfoAsync()).vaultDeviceUuid
    } catch (e: unknown) {
      lastError.value = errString(e)
    }
    if (!integrityBusy.value) {
      integrityDialog.value = null
      integrityActionError.value = null
    }
    const status = await chat.modelLoadStatusAsync()
    if (status) applyLoadStatus(status)
    await refreshActiveModel()
    await refreshActiveLocalModel()
    await refreshInstalledAndCatalog()
    await refreshProviders()
    await restoreLastModel()
  }

  /**
   * Session start (spec 002 §FR-014): once every list above is fresh,
   * loads the model the backend resolver picks — the last model the person
   * picked on this device, while it can load. There is no default model and
   * nothing is picked on the person's behalf. A newly-connected
   * provider's models only exist in that resolver's view *after*
   * `refreshProviders()` has fetched and cached them once, which is why
   * this runs here, once the lists are fresh, and not at the vault open.
   * A local pick is loaded into memory by this load, on first use. A `null`
   * result (`source: 'none'`) means nothing is loadable yet, so this is a
   * no-op; the composer's own model control remains the only place to
   * pick one by hand.
   */
  function shouldSkipAutoLoad(): boolean {
    return (
      activeModel.value !== null ||
      modelLoadPending.value ||
      loadingPhase.value !== null
    )
  }

  async function restoreLastModel() {
    if (shouldSkipAutoLoad()) return
    let result: ResolveDefaultModelResult
    try {
      result = await resolveDefaultModelAsync()
    } catch (e) {
      // A genuine resolver failure (backend/IPC error) must not look
      // identical to "nothing is loadable yet" — the latter is a normal,
      // silent no-op; the former is worth surfacing so it doesn't get
      // mistaken for an empty vault during triage.
      lastError.value = errString(e)
      return
    }
    // Re-check: the resolver call above is a real IPC/DB round trip, and
    // something else — a manual pick from the composer, an activation
    // from the settings — may have started or finished a load while we
    // were awaiting it.
    if (!result.modelId || shouldSkipAutoLoad()) return
    await loadModel(result.modelId)
  }

  const unlisteners: UnlistenFn[] = []
  let listenerConsumers = 0
  let listenerGeneration = 0
  let listenerRegistration: Promise<void> | null = null

  /**
   * Subscribes to the model-load + download-progress events. Mirrors the
   * "registration can finish after the page already stopped listening"
   * handling the chat page uses for its own events: `stopListening` may run
   * before one of these promises resolves, in which case the late unlisten
   * is disposed immediately instead of leaking.
   */
  async function startListening() {
    listenerConsumers += 1
    if (listenerRegistration) {
      await listenerRegistration
      return
    }
    if (listenerConsumers > 1) return
    const generation = ++listenerGeneration
    const registration: Promise<void> = Promise.all(
      [
        chat.onModelLoadProgress(applyLoadProgress),
        chat.onModelLoadStatus(applyLoadStatus),
        chat.onModelLoadError(applyLoadError),
        models.onDownloadProgress((e) => {
          if (downloadingId.value === e.modelId) {
            downloadProgressBytes.value = e.bytesDownloaded
            downloadTotalBytes.value = e.bytesTotal
          }
        }),
        models.onToolUseUpdated(({ modelId }) => {
          if (!installedModels.value.some((model) => model.id === modelId))
            return
          void refreshInstalledAndCatalog().catch((e: unknown) => {
            lastError.value = errString(e)
          })
        }),
      ].map(async (subscription) => {
        const unlisten = await subscription
        if (generation !== listenerGeneration || listenerConsumers === 0)
          unlisten()
        else unlisteners.push(unlisten)
      }),
    ).then(() => undefined)
    listenerRegistration = registration
    await registration
    if (listenerRegistration === registration) listenerRegistration = null
  }

  // Providers and their cached models come from the vault, so another device or window can
  // change them (spec 024 FR-032). Reads the local cache only, so it writes nothing itself.
  onVaultTablesChanged(['providers', 'models'], async () => {
    try {
      await refreshProviders()
    } catch (e: unknown) {
      lastError.value = errString(e)
    }
  })

  /** Stops future subscriptions and disposes every active listener. */
  function stopListening() {
    if (listenerConsumers === 0) return
    listenerConsumers -= 1
    if (listenerConsumers > 0) return
    listenerGeneration += 1
    // A new tab may mount before an old async subscription registration resolves. Detach the
    // in-flight generation so that the new consumer starts a fresh set of listeners.
    listenerRegistration = null
    for (const unlisten of unlisteners.splice(0)) unlisten()
  }

  return {
    activeModel,
    installedModels,
    catalogEntries,
    providerList,
    providerModels,
    modelLoadPending,
    lastError,
    downloadingId,
    downloadProgressBytes,
    downloadTotalBytes,
    loadingPhase,
    loadingModelName,
    loadingProviderName,
    loadErrorModelId,
    integrityDialog,
    integrityBusy,
    integrityActionError,
    noModelsInstalled,
    activeModelId,
    activeLocalModel,
    activeLocalModelId,
    displayModelId,
    displayModelName,
    displayModelCapabilities,
    effortLevel,
    effortOptions,
    effortState,
    updateEffortLevel,
    modelGroups,
    loadingLabel,
    refreshInstalledAndCatalog,
    refreshProviders,
    refreshActiveModel,
    refreshActiveLocalModel,
    loadModel,
    activateLocalModel,
    deactivateLocalModel,
    onIntegrityLoadUntrusted,
    onIntegrityRepairSource,
    onIntegrityChooseOther,
    onIntegrityDialogOpenChange,
    downloadCatalogEntry,
    retryModelLoad,
    initialize,
    startListening,
    stopListening,
  }
})
