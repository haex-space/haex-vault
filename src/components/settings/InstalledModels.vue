<script setup lang="ts">
/**
 * Installed models (spec 005; the upper part of the location `/ai/local`, spec 023 research R10):
 * one select over the models on this device, with badges for the active one and for an update,
 * and the buttons next to it that act on the chosen model: activate (load into memory) or
 * unload, install an update, delete. Below it the details of the chosen model. Choosing a model
 * activates nothing: of the installed models at most one is active, and it is the only local
 * model the chat offers. The list, the active model and the integrity flow are
 * `useModelsStore()` state shared with the chat.
 */
import { toast } from 'vue-sonner'

import type { HuggingFaceUpdateStatus } from '~/composables/useHuggingFace'
import type { SettingsSelectOption } from '~/components/settings/Select.vue'
import { humanBytes } from '~/lib/models/format'

const { t } = useI18n()
const checkUpdates = useActionOrThrow('settings.models.checkUpdates')
const installUpdate = useActionOrThrow('settings.models.installUpdate')
const cancelDownload = useActionOrThrow('settings.models.cancelDownload')
const deleteModel = useActionOrThrow('settings.models.delete')
const activate = useAction('settings.models.activate')
const deactivate = useAction('settings.models.deactivate')
const decideIntegrity = useAction('chat.modelIntegrity.decide')
const downloads = useModelDownloadsStore()

const modelStore = useModelsStore()
const {
  installedModels: installed,
  activeLocalModelId,
  integrityDialog,
  integrityBusy,
  integrityActionError,
} = storeToRefs(modelStore)
const {
  refreshInstalledAndCatalog,
  refreshActiveLocalModel,
  onIntegrityDialogOpenChange,
} = modelStore

const loading = ref(true)

/** Errors show as a toast: the reason, and the detail of a HuggingFace or file error under it. */
function toastHfError(e: unknown) {
  const detail = hfErrorDetail(e)
  toast.error(t(hfErrorKey(e)), detail ? { description: detail } : undefined)
}

const updateStatuses = ref<Record<string, HuggingFaceUpdateStatus>>({})

/** The model the buttons act on: the one chosen in the select, at first the active one. */
const selectedId = ref('')
const pickedByPerson = ref(false)
const selected = computed(
  () => installed.value.find((model) => model.id === selectedId.value) ?? null,
)

function pick(id: string) {
  selectedId.value = id
  pickedByPerson.value = true
}

// A model that went away (deleted) leaves the choice on the active one, else the first.
watch(
  installed,
  (list) => {
    if (list.some((model) => model.id === selectedId.value)) return
    selectedId.value =
      list.find((model) => model.id === activeLocalModelId.value)?.id ??
      list[0]?.id ??
      ''
  },
  { immediate: true },
)
// The active model may be loaded after this view opened (the vault-open preload): until the
// person chose something, the select follows it.
watch(activeLocalModelId, (id) => {
  if (id && !pickedByPerson.value) selectedId.value = id
})

const options = computed<SettingsSelectOption[]>(() =>
  installed.value.map((model) => ({
    value: model.id,
    label: model.name,
    badges: [
      ...(activeLocalModelId.value === model.id
        ? [{ text: t('models.installed.active'), tone: 'success' as const }]
        : []),
      ...(updateStatuses.value[model.id]?.updateAvailable
        ? [{ text: t('models.update.available'), tone: 'warning' as const }]
        : []),
    ],
  })),
)
const checkingUpdates = ref(false)
const installingUpdateId = ref<string | null>(null)

// The model of a running activation, unload or delete, and which of them it is: only the button
// that was pressed spins. The others wait disabled, for every model: the chat holds one load at a
// time, and choosing another model meanwhile must not start a second one.
const busyModelId = ref<string | null>(null)
const busyAction = ref<'activate' | 'unload' | 'delete' | null>(null)

async function reloadAsync() {
  loading.value = true
  try {
    await Promise.all([refreshInstalledAndCatalog(), refreshActiveLocalModel()])
  } catch (e) {
    toastHfError(e)
  } finally {
    loading.value = false
  }
}

async function checkUpdatesNowAsync() {
  checkingUpdates.value = true
  try {
    const { statuses } = (await checkUpdates()) as {
      statuses: HuggingFaceUpdateStatus[]
    }
    updateStatuses.value = Object.fromEntries(
      statuses.map((s) => [s.modelId, s]),
    )
    // A model whose check failed: one toast for the whole check.
    if (statuses.some((s) => s.errorCode)) toast.error(t('models.update.error'))
  } catch (e) {
    toastHfError(e)
  } finally {
    checkingUpdates.value = false
  }
}

/**
 * The model whose update waits for the confirmation (spec 043 FR-028); the update is the same file
 * in a newer revision, so the installed size stands in for its size.
 */
const confirmingUpdate = ref<(typeof installed.value)[number] | null>(null)
const confirmUpdateOpen = computed({
  get: () => confirmingUpdate.value !== null,
  set: (value: boolean) => {
    if (!value) confirmingUpdate.value = null
  },
})

function updateConfirmed() {
  const model = confirmingUpdate.value
  confirmingUpdate.value = null
  if (model) void installUpdateForAsync(model.id)
}

async function installUpdateForAsync(modelId: string) {
  installingUpdateId.value = modelId
  try {
    await installUpdate({ modelId })
    await reloadAsync()
    await checkUpdatesNowAsync()
  } catch (e) {
    // A cancelled update is what the person asked for, not an error.
    if (!isDownloadCancelled(e)) toastHfError(e)
  } finally {
    installingUpdateId.value = null
    downloads.clearDownload(modelId)
  }
}

async function cancelUpdateAsync(modelId: string) {
  try {
    await cancelDownload({ modelId })
  } catch (e) {
    toastHfError(e)
  }
}

async function deleteModelAsync(id: string) {
  if (!confirm(t('models.installed.deleteConfirm'))) return
  busyModelId.value = id
  busyAction.value = 'delete'
  try {
    await deleteModel({ modelId: id })
    await reloadAsync()
  } catch (e) {
    toast.error(t(hfErrorKey(e)))
  } finally {
    busyModelId.value = null
    busyAction.value = null
  }
}

/** The activation and any integrity failure are handled by the store's `activateLocalModel`; a
 * failed integrity check opens the dialog below, any other failure (such as a model that does not
 * fit the hardware) comes back from the action as an error, already display-ready, and is
 * toasted. */
async function activateAsync(id: string) {
  busyModelId.value = id
  busyAction.value = 'activate'
  try {
    await activate({ modelId: id })
  } catch (e) {
    // The integrity dialog is the answer to a failed check; the action reports it as a failure
    // to its other callers.
    if (!integrityDialog.value) {
      toast.error(e instanceof Error ? e.message : String(e))
    }
  } finally {
    busyModelId.value = null
    busyAction.value = null
  }
}

async function unloadAsync(id: string) {
  busyModelId.value = id
  busyAction.value = 'unload'
  try {
    await deactivate({})
  } catch (e) {
    toast.error(e instanceof Error ? e.message : String(e))
  } finally {
    busyModelId.value = null
    busyAction.value = null
  }
}

/** A repair re-downloads under the same model id; its finished progress entry goes with it. */
async function onRepairSourceAsync() {
  const modelId = integrityDialog.value?.modelId
  await decideIntegrity({ decision: 'repairSource' })
  if (modelId) downloads.clearDownload(modelId)
}

onMounted(async () => {
  await reloadAsync()
  // Opening this view is an explicit, user-visible update check; the command is read-only and
  // only checks models with a stored HuggingFace ref.
  await checkUpdatesNowAsync()
})
</script>

<template>
  <section class="flex flex-col gap-3">
    <div class="flex justify-end">
      <UiButton
        type="button"
        variant="outline"
        size="sm"
        :loading="checkingUpdates"
        @click="checkUpdatesNowAsync"
      >
        {{ t('models.update.checkNow') }}
      </UiButton>
    </div>

    <div v-if="loading" class="text-sm text-muted-foreground">
      {{ t('models.search.loading') }}
    </div>
    <p
      v-else-if="installed.length === 0"
      class="px-1 text-sm text-muted-foreground"
      data-testid="settings-installed-empty"
    >
      {{ t('models.installed.empty') }}
    </p>

    <SettingsGroup v-if="selected">
      <SettingsRow
        :title="t('settings.localModels.model')"
        label-for="local-model-select"
      >
        <template #backdrop>
          <ModelsDownloadBar :model-id="selected.id" />
        </template>
        <SettingsSelect
          id="local-model-select"
          class="w-72 md:w-96"
          :model-value="selectedId"
          :options="options"
          data-testid="settings-local-model-select"
          @update:model-value="pick"
        />
        <UiButton
          v-if="activeLocalModelId === selected.id"
          type="button"
          size="sm"
          variant="outline"
          :disabled="busyModelId !== null"
          :loading="busyModelId === selected.id && busyAction === 'unload'"
          @click="unloadAsync(selected.id)"
        >
          {{ t('models.installed.unload') }}
        </UiButton>
        <UiButton
          v-else
          type="button"
          size="sm"
          :disabled="busyModelId !== null"
          :loading="busyModelId === selected.id && busyAction === 'activate'"
          @click="activateAsync(selected.id)"
        >
          {{ t('models.installed.activate') }}
        </UiButton>
        <UiButton
          v-if="installingUpdateId === selected.id"
          type="button"
          size="sm"
          variant="outline"
          :data-testid="`models-cancel-update-${selected.id}`"
          @click="cancelUpdateAsync(selected.id)"
        >
          {{ t('models.filePicker.cancel') }}
        </UiButton>
        <UiButton
          v-else
          type="button"
          size="sm"
          variant="outline"
          :disabled="!updateStatuses[selected.id]?.updateAvailable"
          @click="confirmingUpdate = selected"
        >
          {{ t('models.update.install') }}
        </UiButton>
        <DeleteButton
          :label="`${t('models.installed.delete')}: ${selected.name}`"
          :disabled="busyModelId !== null"
          :loading="busyModelId === selected.id && busyAction === 'delete'"
          @click="deleteModelAsync(selected.id)"
        />
      </SettingsRow>
      <SettingsRow>
        <template #description>
          <span class="flex flex-col gap-0.5 text-xs">
            <span class="flex flex-wrap gap-x-3 gap-y-0.5">
              <span>{{
                t(`models.installed.source.${selected.sourceKind}`)
              }}</span>
              <span>{{
                t(`models.installed.integrity.${selected.integrityStatus}`)
              }}</span>
              <span>{{ humanBytes(selected.sizeBytes) }}</span>
            </span>
            <ModelsDownloadStatus :model-id="selected.id" />
            <span
              v-if="
                selected.hfRevisionRef &&
                updateStatuses[selected.id] &&
                !updateStatuses[selected.id]?.errorCode
              "
            >
              <span
                v-if="updateStatuses[selected.id]?.updateAvailable"
                class="text-warning"
              >
                {{ t('models.update.available') }} ({{
                  t('models.update.oldRevision')
                }}:
                {{ updateStatuses[selected.id]?.installedRevision.slice(0, 8) }}
                → {{ t('models.update.newRevision') }}:
                {{ updateStatuses[selected.id]?.latestRevision?.slice(0, 8) }})
              </span>
              <span v-else>{{ t('models.update.upToDate') }}</span>
            </span>
            <span
              v-else-if="
                selected.sourceKind === 'huggingface' && !selected.hfRevisionRef
              "
            >
              {{ t('models.update.notTrackable') }}
            </span>
          </span>
        </template>
      </SettingsRow>
    </SettingsGroup>

    <!-- The context of the chosen model (spec 086). A change for the loaded model loads it again. -->
    <SettingsModelContextSetting
      v-if="selected"
      :model-id="selected.id"
      :active="activeLocalModelId === selected.id"
      :busy="busyModelId !== null"
      @changed="activateAsync(selected.id)"
    />

    <ModelsDownloadConfirmStep
      v-model:open="confirmUpdateOpen"
      :target="
        confirmingUpdate && {
          kind: 'file',
          sizeBytes: confirmingUpdate.sizeBytes,
        }
      "
      :name="confirmingUpdate?.name ?? ''"
      @confirm="updateConfirmed"
    />

    <ModelsModelIntegrityDialog
      v-if="integrityDialog"
      :open="integrityDialog !== null"
      :error-kind="integrityDialog.errorKind"
      :expected-sha256="integrityDialog.expected"
      :actual-sha256="integrityDialog.actual"
      :busy="integrityBusy"
      :action-error="integrityActionError"
      @update:open="onIntegrityDialogOpenChange"
      @load-untrusted="decideIntegrity({ decision: 'loadUntrusted' })"
      @repair-source="onRepairSourceAsync"
      @choose-other="decideIntegrity({ decision: 'chooseOther' })"
    />
  </section>
</template>
