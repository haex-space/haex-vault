<script setup lang="ts">
/**
 * Imports pass files one by one (spec 092, FR-028 to FR-033). The files wait in the import store;
 * each is read by the backend into memory, shown with the proposal of FR-019, and saved or skipped
 * by the person. A file that cannot be read is named and the next one goes on (one failing does not
 * stop the others). A pass the vault holds already asks: skip or add anyway. At the end the
 * report names each pass, its place and what was not kept, and can be saved as text. Nothing here
 * holds a code value (FR-025, FR-032).
 */
import { toast } from 'vue-sonner'
import type { PassImportPreview } from '@bindings/PassImportPreview'
import type { PassImportReport } from '@bindings/PassImportReport'
import type { PassOnDuplicate } from '@bindings/PassOnDuplicate'
import type { PickedFile } from '@bindings/PickedFile'
import { pathFor, pathText, tagOrNull } from '~/lib/wallet/importPlace'
import type { ImportDraft } from './ImportForm.vue'

const { t, locale } = useI18n()
const { errString } = useErrorString()
const queue = useWalletImportStore()
const { previewAsync, saveAsync, cancelAsync, saveReportAsync } =
  usePassImport()
const { pickSaveTargetAsync } = usePickedFile()

const preview = ref<PassImportPreview | null>(null)
const draft = ref<ImportDraft | null>(null)
const reports = ref<PassImportReport[]>([])
const total = ref(0)
const done = ref(0)
const reading = ref(false)
const saving = ref(false)
const finished = ref(false)
/** Taking the next file runs once at a time. */
let advancing = false
/** Counts the imports that were closed: an answer that arrives for a closed one is dropped. */
let session = 0

const open = computed(
  () =>
    preview.value !== null ||
    reading.value ||
    finished.value ||
    queue.files.length > 0,
)

/** The language of the texts of a pass: the window's, without the region. */
const language = computed(() => locale.value.split('-')[0] ?? 'en')

watch(
  () => queue.files.length,
  (length, before = 0) => {
    if (length > before) total.value += length - before
    void advanceAsync()
  },
  { immediate: true },
)

async function advanceAsync() {
  if (advancing || preview.value !== null) return
  advancing = true
  const mine = session
  try {
    for (let file = queue.take(); file !== null; file = queue.take()) {
      const shown = await readOneAsync(file, mine)
      if (mine !== session) return
      if (shown) return
      done.value += 1
    }
    finished.value = reports.value.length > 0
    if (!finished.value) total.value = 0
  } finally {
    advancing = false
    // Files came in while a closed import was still being read.
    if (mine !== session && queue.files.length > 0) void advanceAsync()
  }
}

/** Reads a file into the form; false when it could not be read (the next one goes on). */
async function readOneAsync(file: PickedFile, mine: number): Promise<boolean> {
  reading.value = true
  try {
    const read = await previewAsync(file, language.value)
    if (mine !== session) {
      // The import was closed while the file was read: the backend keeps nothing for it.
      await cancelAsync(read.previewId).catch(() => undefined)
      return false
    }
    const proposal = read.proposal
    preview.value = read
    draft.value = {
      title: read.pass.title,
      passKind: read.pass.passKind,
      folder: pathText(proposal.folderPath ?? []),
      tag: proposal.tagName ?? '',
      parentId: null,
      zone:
        read.pass.start && 'at' in read.pass.start ? read.pass.start.zone : '',
    }
    return true
  } catch (cause) {
    toast.error(errString(cause))
    return false
  } finally {
    reading.value = false
  }
}

async function nextAsync() {
  done.value += 1
  preview.value = null
  draft.value = null
  await advanceAsync()
}

async function saveOneAsync(onDuplicate: PassOnDuplicate) {
  const current = preview.value
  const form = draft.value
  if (current === null || form === null || saving.value) return
  saving.value = true
  const mine = session
  try {
    const start = current.pass.start
    const saved = await saveAsync(current.previewId, {
      title: form.title,
      passKind: form.passKind,
      start:
        start && 'at' in start && form.zone.trim() !== ''
          ? { at: start.at, zone: form.zone.trim() }
          : undefined,
      folderPath: pathFor(form.folder, current.proposal.folderPath),
      tagName: tagOrNull(form.tag) ?? undefined,
      parentId: form.parentId ?? undefined,
      onDuplicate,
    })
    await usePasswordsStore().quietReloadAsync()
    // Closed while saving: the pass is saved, the closed import shows nothing of it.
    if (mine !== session) return
    if (saved.id !== undefined && saved.id !== null) {
      reports.value = [...reports.value, saved.report]
    }
    await nextAsync()
  } catch (cause) {
    toast.error(errString(cause))
  } finally {
    saving.value = false
  }
}

async function skipFileAsync() {
  const current = preview.value
  if (current !== null) {
    try {
      await cancelAsync(current.previewId)
    } catch {
      // The preview was already gone; there is nothing to drop.
    }
  }
  await nextAsync()
}

/** Closing ends the whole import: what waits is dropped, and so is the file being looked at. */
async function closeAsync() {
  session += 1
  const current = preview.value
  queue.clear()
  preview.value = null
  draft.value = null
  reports.value = []
  finished.value = false
  total.value = 0
  done.value = 0
  if (current !== null) {
    try {
      await cancelAsync(current.previewId)
    } catch {
      // Gone already.
    }
  }
}

async function saveReportToFileAsync() {
  const file = await pickSaveTargetAsync('wallet-import-report.txt', [
    { name: 'Text', extensions: ['txt'] },
  ])
  if (file === null) return
  try {
    await saveReportAsync(reports.value, file)
  } catch (cause) {
    toast.error(errString(cause))
  }
}

const isDuplicate = computed(() => preview.value?.duplicateOf != null)
const position = computed(() => Math.min(done.value + 1, total.value))
</script>

<template>
  <UiDrawerModal
    :open="open"
    :title="t('wallet.import.title')"
    @update:open="(value: boolean) => !value && closeAsync()"
  >
    <template #content>
      <div class="flex flex-col gap-4" data-testid="wallet-import-dialog">
        <p
          v-if="reading && preview === null"
          class="py-6 text-center text-muted-foreground"
          data-testid="wallet-import-reading"
        >
          {{ t('wallet.import.reading') }}
        </p>

        <template v-else-if="preview !== null && draft !== null">
          <p v-if="total > 1" class="text-sm text-muted-foreground">
            {{ t('wallet.import.progress', { current: position, total }) }}
          </p>
          <div
            v-if="isDuplicate"
            class="rounded-lg border border-border bg-muted p-3 text-sm"
            role="status"
            data-testid="wallet-import-duplicate"
          >
            {{
              t('wallet.import.duplicate', {
                title: preview.duplicateOf?.title ?? t('passwords.untitled'),
              })
            }}
          </div>
          <WalletImportForm
            :key="preview.previewId"
            v-model:draft="draft"
            :preview="preview"
          />
          <div class="flex flex-wrap justify-end gap-2">
            <UiButton
              type="button"
              variant="outline"
              :disabled="saving"
              data-testid="wallet-import-skip"
              @click="isDuplicate ? saveOneAsync('skip') : skipFileAsync()"
            >
              {{
                isDuplicate
                  ? t('wallet.import.skipDuplicate')
                  : t('passwords.cancel')
              }}
            </UiButton>
            <UiButton
              v-if="isDuplicate"
              type="button"
              :loading="saving"
              data-testid="wallet-import-add"
              @click="saveOneAsync('add')"
            >
              {{ t('wallet.import.addAnyway') }}
            </UiButton>
            <UiButton
              v-else
              type="button"
              :loading="saving"
              data-testid="wallet-import-save"
              @click="saveOneAsync('ask')"
            >
              {{ t('passwords.save') }}
            </UiButton>
          </div>
        </template>

        <template v-else-if="finished">
          <p class="text-sm" data-testid="wallet-import-done">
            {{ t('wallet.import.done', { count: reports.length }) }}
          </p>
          <SettingsGroup>
            <li
              v-for="(report, index) in reports"
              :key="index"
              class="flex flex-col px-4 py-3 text-sm"
              data-testid="wallet-import-report-row"
            >
              <span>{{ report.title }}</span>
              <span class="text-xs text-muted-foreground">
                {{
                  report.folderPath.length
                    ? pathText(report.folderPath)
                    : t('wallet.import.topLevel')
                }}
                <template v-if="report.notKept.length">
                  ·
                  {{
                    t('wallet.import.notKeptCount', {
                      count: report.notKept.length,
                    })
                  }}
                </template>
              </span>
            </li>
          </SettingsGroup>
          <div class="flex flex-wrap justify-end gap-2">
            <UiButton
              type="button"
              variant="outline"
              data-testid="wallet-import-report-save"
              @click="saveReportToFileAsync"
            >
              {{ t('wallet.import.saveReport') }}
            </UiButton>
            <UiButton
              type="button"
              data-testid="wallet-import-close"
              @click="closeAsync"
            >
              {{ t('wallet.import.close') }}
            </UiButton>
          </div>
        </template>
      </div>
    </template>
  </UiDrawerModal>
</template>
