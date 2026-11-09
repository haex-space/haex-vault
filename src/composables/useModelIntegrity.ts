import { ref, type Ref } from 'vue'
import type { LoadedModelInfo, useChat } from '~/composables/useChat'
import type { Translate } from '~/composables/useModelInventory'
import {
  parseModelIntegrityFailure,
  type InstalledModel,
  type ModelIntegrityFailure,
  type useModels,
} from '~/composables/useModels'

export interface ModelIntegrityDeps {
  chat: ReturnType<typeof useChat>
  models: ReturnType<typeof useModels>
  t: Translate
  errString: (error: unknown) => string
  installedModels: Ref<InstalledModel[]>
  /** The active local model, which the integrity dialog's "load anyway" activates. */
  activeLocalModel: Ref<LoadedModelInfo | null>
  modelLoadPending: Ref<boolean>
  findModelName: (id: string) => string
  beginLoadingModel: (id: string, name: string) => number
  clearLoadingModel: (token?: number) => void
  refreshInstalledAndCatalog: () => Promise<void>
  /** Reports a non-fatal failure through the owning store's error state. */
  setError: (message: string) => void
}

/**
 * The pre-load integrity dialog: state plus the four decisions a user can
 * make in it, extracted from `useModelsStore` (spaex 500-LoC boundary). The
 * store keeps the load lifecycle itself and hands in the hooks these
 * handlers need to take part in it.
 */
export function useModelIntegrity(deps: ModelIntegrityDeps) {
  const {
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
  } = deps

  const integrityDialog = ref<ModelIntegrityFailure | null>(null)
  const integrityBusy = ref(false)
  const integrityActionError = ref<string | null>(null)

  /**
   * Detects the three structured integrity error kinds `load_model` can
   * return (spec 005 §"load_model und lokale Integritätsprüfung") and opens
   * the decision dialog instead of showing a plain error string. Returns
   * `false` for every other error so the caller falls back to `lastError`.
   */
  function openIntegrityDialog(modelId: string, e: unknown): boolean {
    const failure = parseModelIntegrityFailure(modelId, e)
    if (!failure) return false
    integrityDialog.value = failure
    return true
  }

  /** "Trotzdem als unsicher laden" — bypasses the hash check for this activation only. */
  async function onIntegrityLoadUntrusted() {
    if (!integrityDialog.value) return
    const modelId = integrityDialog.value.modelId
    modelLoadPending.value = true
    const loadToken = beginLoadingModel(modelId, findModelName(modelId))
    integrityBusy.value = true
    integrityActionError.value = null
    try {
      activeLocalModel.value =
        await chat.activateLocalModelWithIntegrityOverrideAsync(modelId)
      integrityDialog.value = null
      try {
        await refreshInstalledAndCatalog()
      } catch (e) {
        // The override load itself already succeeded and the dialog is
        // gone by now — this failure has nowhere left to render as
        // `integrityActionError`, so it goes through the page's general
        // error banner instead of being silently dropped.
        setError(errString(e))
      }
    } catch (e) {
      integrityActionError.value = errString(e)
    } finally {
      integrityBusy.value = false
      modelLoadPending.value = false
      clearLoadingModel(loadToken)
    }
  }

  /** "Erneut herunterladen / neu importieren" — re-installs from the model's stored HF source. */
  async function onIntegrityRepairSource() {
    const dialog = integrityDialog.value
    if (!dialog) return
    const model = installedModels.value.find((m) => m.id === dialog.modelId)
    integrityBusy.value = true
    integrityActionError.value = null
    try {
      if (
        model?.sourceKind === 'huggingface' &&
        model.hfRepo &&
        model.hfFilename
      ) {
        await models.downloadFromHfAsync({
          repoId: model.hfRepo,
          filename: model.hfFilename,
          // Repair reinstalls the stored source: the tracked ref when there
          // is one, otherwise the pinned commit — never an implicit `main`.
          revision: model.hfRevisionRef ?? model.hfRevision ?? undefined,
          name: model.name,
          contextWindow: model.contextWindow,
          // Without this the backend's same-source short-circuit returns the
          // existing row and the corrupt file is never replaced.
          forceRepair: true,
        })
        integrityDialog.value = null
        try {
          await refreshInstalledAndCatalog()
        } catch (e) {
          // Same rationale as `onIntegrityLoadUntrusted`: the repair itself
          // already succeeded and the dialog is already gone, so a refresh
          // failure here goes through the general error banner instead of
          // a now-unreachable `integrityActionError`.
          setError(errString(e))
        }
      } else {
        integrityActionError.value = t('models.integrityDialog.actionFailed')
      }
    } catch (e) {
      integrityActionError.value = errString(e)
    } finally {
      integrityBusy.value = false
    }
  }

  /** "Anderes Modell auswählen" — just closes the dialog; the picker is already visible. */
  function onIntegrityChooseOther() {
    integrityDialog.value = null
  }

  /** Clears integrity-dialog state when the dialog is dismissed. */
  function onIntegrityDialogOpenChange(open: boolean) {
    if (!open) {
      integrityDialog.value = null
      integrityActionError.value = null
    }
  }

  return {
    integrityDialog,
    integrityBusy,
    integrityActionError,
    openIntegrityDialog,
    onIntegrityLoadUntrusted,
    onIntegrityRepairSource,
    onIntegrityChooseOther,
    onIntegrityDialogOpenChange,
  }
}
