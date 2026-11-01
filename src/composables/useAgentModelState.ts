import { computed, type Ref } from 'vue'

/**
 * The agent conversation's model wiring, moved out of `ChatApp.vue` for the 500-line limit when
 * spec 072 added contact conversations beside it: the model store's state, the model actions
 * (spec 020 FR-024), when the composer may send, whether the model is a delegate (spec 009) and
 * the one error banner that merges the page's and the store's errors.
 */
export function useAgentModelState(deps: {
  busy: Ref<boolean>
  streamingMessageId: Ref<string | null>
  lastError: Ref<string | null>
  autonomyPreferenceLoading: Ref<boolean>
  autonomyPreferenceError: Ref<string | null>
}) {
  const modelStore = useModelsStore()
  const refs = storeToRefs(modelStore)
  const {
    activeModel,
    modelLoadPending,
    loadingPhase,
    noModelsInstalled,
    providerList,
    lastError: modelLastError,
  } = refs

  /** Whether the active model resolves to a `cli_delegate` provider — the same provider lookup
   * `MessageList.vue`'s `delegateAnsweredByLabel` does (spec 009: the autonomy control only makes
   * sense for a delegate backend). */
  const isDelegateModel = computed(() => {
    const modelId = activeModel.value?.modelId ?? null
    if (!modelId) return false
    const [providerId] = modelId.split(':')
    return providerList.value.some(
      (p) => p.id === providerId && p.kind === 'cli_delegate',
    )
  })

  const composerInputDisabled = computed(
    () =>
      deps.busy.value &&
      !modelLoadPending.value &&
      deps.streamingMessageId.value === null,
  )
  const sendDisabled = computed(
    () =>
      !activeModel.value ||
      deps.busy.value ||
      modelLoadPending.value ||
      loadingPhase.value !== null ||
      (isDelegateModel.value &&
        (deps.autonomyPreferenceLoading.value ||
          deps.autonomyPreferenceError.value !== null)),
  )

  // lastError and modelStore.lastError are separate refs (Pinia setup stores take no page-local
  // params) — merged into one banner.
  const displayedError = computed(
    () => deps.lastError.value || modelLastError.value,
  )
  function dismissError() {
    deps.lastError.value = null
    modelLastError.value = null
  }

  // Only the message area falls back to the catalog-download state, and only when no model is
  // installed/configured anywhere (spec 002 §FR-014 covers picking among models that DO exist).
  const showModelSelection = computed(() => noModelsInstalled.value)

  return {
    modelStore,
    ...refs,
    isDelegateModel,
    composerInputDisabled,
    sendDisabled,
    displayedError,
    dismissError,
    showModelSelection,
    onIntegrityDialogOpenChange: modelStore.onIntegrityDialogOpenChange,
    // Spec 020 FR-024: model retry and integrity decisions run their catalog actions.
    retryLoad: useAction('chat.model.retryLoad'),
    decideIntegrity: useAction('chat.modelIntegrity.decide'),
  }
}
