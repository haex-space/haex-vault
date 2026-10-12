<script setup lang="ts">
/**
 * A storage on a connection (spec 038, US1, T028, FR-002, FR-004; spec 050 US1): its name and
 * bucket, or folder on WebDAV. A new storage or a new bucket or folder is tested before it is
 * saved; a new name is not. A missing folder is made only when the user says so (FR-006), and a
 * folder inside another storage's folder of the connection gets a warning (FR-003). "Jetzt testen" runs the
 * test again and remembers its outcome on this device; a test object holzi could not delete is
 * named (SC-004). The credentials come from the connection, so none are typed here.
 */
import { invoke } from '@tauri-apps/api/core'
import type { StorageInput } from '@bindings/StorageInput'
import type { StorageView } from '@bindings/StorageView'
import type { TestResult } from '@bindings/TestResult'

const { t } = useI18n()
const router = useTabRouter()
const fieldLabels = useFieldLabels()
const storageError = useStorageError()
const { overview, connection } = useStorageOverview()

const connectionId = computed(() => router.route.params.connectionId ?? '')
const storageId = computed(() => router.route.params.storageId ?? 'new')
const isNew = computed(() => storageId.value === 'new')
const parent = computed(() => connection(connectionId.value))
const existing = computed(() =>
  isNew.value
    ? undefined
    : overview.value?.storages.find((s) => s.id === storageId.value),
)
const missing = computed(
  () =>
    overview.value !== null &&
    (!parent.value || (!isNew.value && !existing.value)),
)

const draft = reactive({ name: '', bucket: '' })
const busy = ref(false)
const failure = ref<string | null>(null)
const tested = ref<string | null>(null)
/** The last save or test found the folder missing (WebDAV, SFTP; FR-006). */
const folderMissing = ref(false)
const tree = computed(() => parent.value?.protocol !== 's3')

let filled = false
watchEffect(() => {
  const stored = existing.value
  if (filled || !stored) return
  filled = true
  draft.name = stored.name
  draft.bucket = stored.bucket
})

async function saveAsync(createFolder = false) {
  if (busy.value) return
  busy.value = true
  failure.value = null
  folderMissing.value = false
  const input: StorageInput = {
    id: isNew.value ? undefined : storageId.value,
    connectionId: connectionId.value,
    name: draft.name.trim() || draft.bucket.trim(),
    bucket: draft.bucket.trim(),
    createFolder,
  }
  try {
    const saved = await invoke<StorageView>('storage_save', { input })
    if (saved.warning) {
      // Saved; the warning stays until the user leaves (FR-003).
      tested.value = t(`settings.storage.warnings.${saved.warning}`)
      return
    }
    router.back()
  } catch (error) {
    failure.value = storageError(error)
    folderMissing.value = tree.value && isFolderMissing(error)
  } finally {
    busy.value = false
  }
}

/** Makes the missing folder: through the save for a new storage or folder, else for the saved one. */
async function createFolderAsync() {
  if (isNew.value || draft.bucket.trim() !== existing.value?.bucket) {
    await saveAsync(true)
    return
  }
  if (busy.value) return
  busy.value = true
  failure.value = null
  folderMissing.value = false
  try {
    show(
      await invoke<TestResult>('storage_folder_create', {
        id: storageId.value,
      }),
    )
  } catch (error) {
    failure.value = storageError(error)
  } finally {
    busy.value = false
  }
}

/** Shows the outcome of a test, naming a test object holzi could not delete (SC-004). */
function show(result: TestResult) {
  const text = t(`settings.storage.outcome.${result.outcome}`)
  const message = result.leftoverKey
    ? `${text}. ${t('settings.storage.leftover', { key: result.leftoverKey })}`
    : text
  if (result.outcome === 'passed') tested.value = message
  else failure.value = message
  folderMissing.value = tree.value && result.outcome === 'folderMissing'
}

async function testAsync() {
  if (busy.value || isNew.value) return
  busy.value = true
  failure.value = null
  tested.value = null
  try {
    show(
      await invoke<TestResult>('storage_test', {
        id: storageId.value,
      }),
    )
  } catch (error) {
    failure.value = storageError(error)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <section class="flex flex-col gap-3" data-testid="storage-form">
    <p v-if="missing" class="px-1 text-sm text-muted-foreground" role="status">
      {{ t('settings.storage.notFound') }}
    </p>

    <form v-else class="flex flex-col gap-3" @submit.prevent="saveAsync()">
      <SettingsGroup :label="parent?.providerName">
        <li class="px-4 py-3">
          <UiInput
            id="storage-name"
            v-model="draft.name"
            :label="t('settings.storage.fields.storageName')"
            :placeholder="draft.bucket"
            :labels="fieldLabels.input.value"
            label-bg="var(--muted)"
            maxlength="80"
            data-testid="storage-name"
          />
        </li>
        <li class="px-4 py-3">
          <UiInput
            id="storage-bucket"
            v-model="draft.bucket"
            :label="
              tree
                ? t('settings.storage.fields.folder')
                : t('settings.storage.fields.bucket')
            "
            :labels="fieldLabels.input.value"
            label-bg="var(--muted)"
            autocomplete="off"
            data-testid="storage-bucket"
          />
        </li>
        <li
          v-if="parent && parent.credentials !== 'present'"
          class="px-4 py-3 text-sm text-destructive"
          role="note"
        >
          {{
            t(`settings.storage.credentialsUnavailable.${parent.credentials}`)
          }}
        </li>
      </SettingsGroup>

      <p
        v-if="tested"
        class="px-1 text-sm text-muted-foreground"
        role="status"
        data-testid="storage-test-passed"
      >
        {{ tested }}
      </p>
      <p
        v-if="failure"
        class="px-1 text-sm text-destructive"
        role="alert"
        data-testid="storage-failure"
      >
        {{ failure }}
      </p>
      <div class="flex flex-wrap gap-2">
        <UiButton type="submit" :loading="busy" data-testid="storage-save">
          {{
            isNew || draft.bucket.trim() !== existing?.bucket
              ? t('settings.storage.save')
              : t('settings.storage.saveName')
          }}
        </UiButton>
        <UiButton
          v-if="folderMissing"
          type="button"
          variant="outline"
          :disabled="busy"
          data-testid="storage-create-folder"
          @click="createFolderAsync"
        >
          {{ t('settings.storage.createFolder') }}
        </UiButton>
        <template v-if="!isNew">
          <UiButton
            type="button"
            variant="outline"
            :disabled="busy"
            data-testid="storage-test"
            @click="testAsync"
          >
            {{ t('settings.storage.test') }}
          </UiButton>
          <UiButton
            type="button"
            variant="outline"
            :disabled="busy"
            data-testid="storage-remove"
            @click="router.push(`/storage/remove/storage/${storageId}`)"
          >
            {{ t('settings.storage.remove') }}
          </UiButton>
        </template>
      </div>
    </form>
  </section>
</template>
