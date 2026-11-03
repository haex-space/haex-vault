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
const listErrorKey = ref<string | null>(null)
const listErrorDetail = ref<string | null>(null)

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
const updateErrorKey = ref<string | null>(null)
const updateErrorDetail = ref<string | null>(null)
const installingUpdateId = ref<string | null>(null)

// The row of a running load or delete, and which of the two it is: only the button that was
// pressed spins, the other one of the row waits disabled.
const busyModelId = ref<string | null>(null)
const busyAction = ref<'activate' | 'unload' | 'delete' | null>(null)
const deleteErrorKey = ref<string | null>(null)
// The store's `lastError` is already display-ready (unlike the i18n keys above):
// `chat.model.select` runs the store's `loadModel`, so its failure reads like the chat's.
const loadErrorMessage = ref<string | null>(null)

async function reloadAsync() {
  loading.value = true
  listErrorKey.value = null
  listErrorDetail.value = null
  try {
    await Promise.all([refreshInstalledAndCatalog(), refreshActiveLocalModel()])
  } catch (e) {
    listErrorKey.value = hfErrorKey(e)
    listErrorDetail.value = hfErrorDetail(e)
  } finally {
    loading.value = false
  }
}

async function checkUpdatesNowAsync() {
  checkingUpdates.value = true
  updateErrorKey.value = null
  updateErrorDetail.value = null
  try {
    const { statuses } = (await checkUpdates()) as {
      statuses: HuggingFaceUpdateStatus[]
    }
    updateStatuses.value = Object.fromEntries(
      statuses.map((s) => [s.modelId, s]),
    )
  } catch (e) {
    updateErrorKey.value = hfErrorKey(e)
    updateErrorDetail.value = hfErrorDetail(e)
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
  updateErrorKey.value = null
  updateErrorDetail.value = null
  try {
    await installUpdate({ modelId })
    await reloadAsync()
    await checkUpdatesNowAsync()
  } catch (e) {
    // A cancelled update is what the person asked for, not an error.
    if (!isDownloadCancelled(e)) {
      updateErrorKey.value = hfErrorKey(e)
      updateErrorDetail.value = hfErrorDetail(e)
    }
  } finally {
    installingUpdateId.value = null
    downloads.clearDownload(modelId)
  }
}

async function cancelUpdateAsync(modelId: string) {
  try {
    await cancelDownload({ modelId })
  } catch (e) {
    updateErrorKey.value = hfErrorKey(e)
    updateErrorDetail.value = hfErrorDetail(e)
  }
}

async function deleteModelAsync(id: string) {
  if (!confirm(t('models.installed.deleteConfirm'))) return
  busyModelId.value = id
  busyAction.value = 'delete'
  deleteErrorKey.value = null
  try {
    await deleteModel({ modelId: id })
    await reloadAsync()
  } catch (e) {
    deleteErrorKey.value = hfErrorKey(e)
  } finally {
    busyModelId.value = null
    busyAction.value = null
  }
}

/** The activation and any integrity failure are handled by the store's `activateLocalModel`,
 * which never throws; it sets `integrityDialog` (the dialog below) or `lastError`. A model that
 * does not fit the hardware fails here, and the row says so. */
async function activateAsync(id: string) {
  busyModelId.value = id
  busyAction.value = 'activate'
  loadErrorMessage.value = null
  try {
    await activate({ modelId: id })
  } catch (e) {
    loadErrorMessage.value = e instanceof Error ? e.message : String(e)
  } finally {
    busyModelId.value = null
    busyAction.value = null
  }
}

async function unloadAsync(id: string) {
  busyModelId.value = id
  busyAction.value = 'unload'
  loadErrorMessage.value = null
  try {
    await deactivate({})
  } catch (e) {
    loadErrorMessage.value = e instanceof Error ? e.message : String(e)
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

    <p v-if="listErrorKey" class="text-sm text-destructive" role="alert">
      {{ t(listErrorKey) }}
      <span v-if="listErrorDetail" class="block text-xs">{{
        listErrorDetail
      }}</span>
    </p>
    <p v-if="updateErrorKey" class="text-sm text-destructive" role="alert">
      {{ t(updateErrorKey) }}
      <span v-if="updateErrorDetail" class="block text-xs">{{
        updateErrorDetail
      }}</span>
    </p>
    <p v-if="loadErrorMessage" class="text-sm text-destructive" role="alert">
      {{ loadErrorMessage }}
    </p>
    <p v-if="deleteErrorKey" class="text-sm text-destructive" role="alert">
      {{ t(deleteErrorKey) }}
    </p>

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
          :disabled="busyModelId === selected.id"
          :loading="busyModelId === selected.id && busyAction === 'unload'"
          @click="unloadAsync(selected.id)"
        >
          {{ t('models.installed.unload') }}
        </UiButton>
        <UiButton
          v-else
          type="button"
          size="sm"
          :disabled="busyModelId === selected.id"
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
        <UiButton
          type="button"
          size="sm"
          variant="ghost"
          :aria-label="`${t('models.installed.delete')}: ${selected.name}`"
          :tooltip="t('models.installed.delete')"
          :disabled="busyModelId === selected.id"
          :loading="busyModelId === selected.id && busyAction === 'delete'"
          @click="deleteModelAsync(selected.id)"
        >
          <Icon name="lucide:trash-2" class="size-4" />
        </UiButton>
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
            <span v-if="selected.hfRevisionRef && updateStatuses[selected.id]">
              <span
                v-if="updateStatuses[selected.id]?.errorCode"
                class="text-destructive"
              >
                {{ t('models.update.error') }}
              </span>
              <span
                v-else-if="updateStatuses[selected.id]?.updateAvailable"
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
