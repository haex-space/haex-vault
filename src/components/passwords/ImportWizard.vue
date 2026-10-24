<script setup lang="ts">
/**
 * The import (spec 034, US7, FR-023) at `/import`: choose the source and the file (KeePass also
 * asks for the password and an optional key file, haex-vault 2.x (Legacy) for its vault password, spec 037),
 * look at what the file holds (counts, folders,
 * trashed entries, history states, attachments, passkeys, duplicates, warnings), choose what to do
 * with duplicates, watch the progress (with cancel; a cancel or a fatal error removes what was
 * written, shown as its own phase), and read the report: the numbers and the places to rework by
 * hand, grouped by entry, with a button that opens the entry and one that saves the report as a
 * text file. Nothing here shows or keeps a secret: the password is cleared once it was used and
 * when the window part goes away.
 *
 * Maintainability exception (spaex 500-LoC rule): the four steps share the chosen source, file
 * and secrets of one run, and most of the length is the template. Concrete split plan, if this
 * grows further: move the template parts of the steps "preview" and "done" (counts, warnings,
 * the grouped report) into `ImportPreviewStep.vue` and `ImportReportStep.vue` with props.
 */
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { toast } from 'vue-sonner'
import type { ImportPreview } from '@bindings/ImportPreview'
import type { ImportReport } from '@bindings/ImportReport'
import type { ImportSource } from '@bindings/ImportSource'
import type { OnDuplicate } from '@bindings/OnDuplicate'
import type { PickedFile } from '@bindings/PickedFile'
import type { Progress } from '@bindings/Progress'
import {
  canPreviewImport,
  groupReport,
  importFailureReason,
  progressPercent,
  sourceSecrets,
} from '~/lib/passwords/importReport'

type Step = 'choose' | 'preview' | 'running' | 'done'

const SOURCES: { id: ImportSource; extensions: string[] }[] = [
  { id: 'keepass', extensions: ['kdbx'] },
  { id: 'bitwarden', extensions: ['json', 'csv'] },
  { id: 'lastpass', extensions: ['csv'] },
  { id: 'legacyhaexvault', extensions: ['db'] },
]
const LIMIT_MIB = 25

const { t } = useI18n()
const { errString } = useErrorString()
const router = useTabRouter()
const store = usePasswordsStore()
const {
  importPreviewAsync,
  importRunAsync,
  importCancelAsync,
  importReportSaveAsync,
} = usePasswords()
const { pickOneAsync, pickSaveTargetAsync, nameOfAsync } = usePickedFile()
const { isAndroid } = useDeviceCapabilities()

const { password: passwordLabels } = useFieldLabels()

const step = ref<Step>('choose')
const source = ref<ImportSource>('keepass')
// The choices of the file dialog and the names to show for them (spec 043: no path is derived).
const file = ref<PickedFile | null>(null)
const fileName = ref('')
const password = ref('')
const keyFile = ref<PickedFile | null>(null)
const keyFileName = ref('')
const onDuplicate = ref<OnDuplicate>('skip')
const preview = ref<ImportPreview | null>(null)
const report = ref<ImportReport | null>(null)
const progress = ref<Progress | null>(null)
const error = ref<string | null>(null)
const busy = ref(false)
const cancelling = ref(false)

const extensions = computed(
  () => SOURCES.find((s) => s.id === source.value)?.extensions ?? [],
)
const secrets = computed(() => sourceSecrets(source.value))
const canPreview = computed(() =>
  canPreviewImport(
    source.value,
    file.value !== null,
    password.value !== '',
    keyFile.value !== null,
  ),
)
const percent = computed(() =>
  progress.value
    ? progressPercent(progress.value.done, progress.value.total)
    : null,
)
const groups = computed(() =>
  report.value ? groupReport(report.value.needsAttention) : [],
)

function args() {
  return {
    source: source.value,
    file: file.value ?? '',
    ...(password.value === '' ? {} : { password: password.value }),
    ...(keyFile.value === null ? {} : { keyFile: keyFile.value }),
  }
}

function chooseSource(next: ImportSource) {
  source.value = next
  file.value = null
  keyFile.value = null
  error.value = null
}

async function pickFileAsync() {
  const selected = await pickOneAsync([
    {
      name: t(`passwords.import.sources.${source.value}`),
      extensions: extensions.value,
    },
  ])
  if (selected === null) return
  fileName.value = await nameOfAsync(selected)
  file.value = selected
  error.value = null
}

async function pickKeyFileAsync() {
  const selected = await pickOneAsync()
  if (selected === null) return
  keyFileName.value = await nameOfAsync(selected)
  keyFile.value = selected
}

async function previewAsync() {
  busy.value = true
  error.value = null
  try {
    preview.value = await importPreviewAsync(args())
    step.value = 'preview'
  } catch (cause) {
    error.value = errString(cause)
  } finally {
    busy.value = false
  }
}

async function runAsync() {
  step.value = 'running'
  progress.value = null
  cancelling.value = false
  error.value = null
  try {
    report.value = await importRunAsync({
      ...args(),
      onDuplicate: onDuplicate.value,
    })
    step.value = 'done'
    await store.quietReloadAsync()
  } catch (cause) {
    if (importFailureReason(cause) === 'cancelled')
      toast.info(t('passwords.import.cancelled'))
    else error.value = errString(cause)
    step.value = 'choose'
  } finally {
    // The password has done its job; it is not kept longer than the run.
    password.value = ''
  }
}

async function cancelAsync() {
  cancelling.value = true
  try {
    await importCancelAsync()
  } catch (cause) {
    toast.error(errString(cause))
  }
}

async function saveReportAsync() {
  if (!report.value) return
  const target = await pickSaveTargetAsync('import-report.txt')
  if (!target) return
  try {
    await importReportSaveAsync(report.value, target)
    toast.success(t('passwords.import.reportSaved'))
  } catch (cause) {
    toast.error(errString(cause))
  }
}

function again() {
  report.value = null
  preview.value = null
  file.value = null
  keyFile.value = null
  step.value = 'choose'
}

function rowText(row: ImportReport['needsAttention'][number]): string {
  return t(`passwords.import.kinds.${row.kind}`, {
    field: row.field ?? '',
    file: row.fileName ?? '',
    size:
      row.sizeMib === null || row.sizeMib === undefined
        ? ''
        : row.sizeMib.toFixed(2),
    limit: LIMIT_MIB,
  })
}

const countKeys = [
  'entries',
  'groups',
  'trashedEntries',
  'historyStates',
  'attachments',
  'passkeys',
  'tags',
  'presets',
  'duplicates',
] as const

let unlisten: UnlistenFn | undefined
onMounted(async () => {
  try {
    unlisten = await listen<Progress>('passwords-import-progress', (event) => {
      progress.value = event.payload
    })
  } catch {
    // No event bridge (a preview in a browser): the run still ends with its report.
  }
})
onBeforeUnmount(() => {
  unlisten?.()
  password.value = ''
})
</script>

<template>
  <div class="min-h-0 flex-1 overflow-y-auto px-4 pb-6 @md:px-6">
    <div class="mx-auto flex w-full max-w-3xl flex-col gap-4">
      <div class="flex items-center gap-2 pt-1">
        <UiButton
          variant="ghost"
          size="icon"
          class="-ml-2 shrink-0"
          :aria-label="t('passwords.back')"
          data-testid="passwords-import-back"
          :disabled="step === 'running'"
          @click="router.back()"
        >
          <Icon name="lucide:arrow-left" class="size-5" />
        </UiButton>
        <h1
          class="min-w-0 flex-1 truncate text-2xl font-bold"
          data-testid="passwords-import-title"
        >
          {{ t('passwords.import.title') }}
        </h1>
      </div>

      <p
        v-if="error"
        class="text-sm text-destructive"
        role="alert"
        data-testid="passwords-import-error"
      >
        {{ error }}
      </p>

      <!-- 1. Source, file and credentials -->
      <template v-if="step === 'choose'">
        <SettingsGroup
          radio
          :label="t('passwords.import.source')"
          :model-value="source"
          @update:model-value="chooseSource($event as ImportSource)"
        >
          <SettingsOptionRow
            v-for="option in SOURCES"
            :key="option.id"
            type="radio"
            :value="option.id"
            :title="t(`passwords.import.sources.${option.id}`)"
            :description="t(`passwords.import.hints.${option.id}`)"
            :data-testid="`passwords-import-source-${option.id}`"
          />
        </SettingsGroup>
        <SettingsGroup :label="t('passwords.import.file')">
          <SettingsRow
            :title="file ? fileName : t('passwords.import.noFile')"
            :description="
              isAndroid && source === 'legacyhaexvault'
                ? t('passwords.import.fileHintAndroidLegacyHaexVault')
                : t('passwords.import.fileHint')
            "
            icon="lucide:file"
          >
            <UiButton
              variant="outline"
              size="sm"
              data-testid="passwords-import-pick"
              @click="pickFileAsync"
            >
              {{ t('passwords.import.chooseFile') }}
            </UiButton>
          </SettingsRow>
          <template v-if="secrets.password || secrets.keyFile">
            <SettingsRow
              v-if="secrets.password"
              :title="t('passwords.import.password')"
              icon="lucide:key-round"
            >
              <UiInputPassword
                id="passwords-import-password"
                v-model="password"
                class="w-56"
                :labels="passwordLabels"
                autocomplete="off"
              />
            </SettingsRow>
            <SettingsRow
              v-if="secrets.keyFile"
              :title="keyFile ? keyFileName : t('passwords.import.noKeyFile')"
              :description="t('passwords.import.keyFileHint')"
              icon="lucide:file-key"
            >
              <UiButton variant="outline" size="sm" @click="pickKeyFileAsync">
                {{ t('passwords.import.chooseKeyFile') }}
              </UiButton>
              <UiButton
                v-if="keyFile"
                variant="ghost"
                size="sm"
                @click="keyFile = null"
              >
                {{ t('passwords.import.removeKeyFile') }}
              </UiButton>
            </SettingsRow>
          </template>
        </SettingsGroup>
        <div class="flex justify-end">
          <UiButton
            :disabled="!canPreview"
            :loading="busy"
            data-testid="passwords-import-preview"
            @click="previewAsync"
          >
            {{ t('passwords.import.showPreview') }}
          </UiButton>
        </div>
      </template>

      <!-- 2. Preview and duplicate choice -->
      <template v-else-if="step === 'preview' && preview">
        <SettingsGroup :label="t('passwords.import.previewTitle')">
          <SettingsRow
            v-for="key in countKeys"
            :key="key"
            :title="t(`passwords.import.counts.${key}`)"
          >
            <span
              class="tabular-nums"
              :data-testid="`passwords-import-count-${key}`"
              >{{ preview[key] }}</span
            >
          </SettingsRow>
        </SettingsGroup>
        <SettingsGroup
          v-if="preview.warnings.length"
          :label="t('passwords.import.warnings')"
        >
          <SettingsRow
            v-for="warning in preview.warnings"
            :key="warning"
            :title="t(`passwords.import.warningKinds.${warning}`)"
            icon="lucide:triangle-alert"
          />
        </SettingsGroup>
        <SettingsGroup
          v-if="preview.duplicates > 0"
          radio
          :model-value="onDuplicate"
          :label="t('passwords.import.onDuplicate')"
          @update:model-value="onDuplicate = $event as OnDuplicate"
        >
          <SettingsOptionRow
            type="radio"
            value="skip"
            :title="t('passwords.import.skip')"
            :description="t('passwords.import.skipHint')"
          />
          <SettingsOptionRow
            type="radio"
            value="create"
            :title="t('passwords.import.create')"
            :description="t('passwords.import.createHint')"
          />
        </SettingsGroup>
        <div class="flex justify-end gap-2">
          <UiButton variant="outline" @click="step = 'choose'">{{
            t('passwords.import.change')
          }}</UiButton>
          <UiButton data-testid="passwords-import-start" @click="runAsync">
            {{ t('passwords.import.start') }}
          </UiButton>
        </div>
      </template>

      <!-- 3. Progress -->
      <template v-else-if="step === 'running'">
        <SettingsGroup :label="t('passwords.import.running')">
          <SettingsRow
            :title="
              progress
                ? t(`passwords.import.phases.${progress.phase}`)
                : t('passwords.import.phases.reading')
            "
            :description="
              progress ? `${progress.done} / ${progress.total}` : undefined
            "
            icon="lucide:loader-circle"
          >
            <UiButton
              variant="outline"
              size="sm"
              :disabled="cancelling || progress?.phase === 'rollback'"
              data-testid="passwords-import-cancel"
              @click="cancelAsync"
            >
              {{ t('passwords.import.cancel') }}
            </UiButton>
          </SettingsRow>
          <li class="px-4 pb-4">
            <div
              class="h-2 overflow-hidden rounded-full bg-background"
              role="progressbar"
              aria-valuemin="0"
              aria-valuemax="100"
              :aria-valuenow="percent ?? undefined"
            >
              <div
                class="h-full bg-primary transition-[width] duration-150"
                :class="percent === null ? 'animate-pulse' : ''"
                :style="{ width: `${percent ?? 35}%` }"
              />
            </div>
          </li>
        </SettingsGroup>
        <p
          v-if="progress?.phase === 'rollback'"
          class="text-sm text-muted-foreground"
          role="status"
        >
          {{ t('passwords.import.rollingBack') }}
        </p>
      </template>

      <!-- 4. Report -->
      <template v-else-if="step === 'done' && report">
        <SettingsGroup :label="t('passwords.import.done')">
          <SettingsRow :title="t('passwords.import.report.imported')">
            <span
              class="tabular-nums"
              data-testid="passwords-import-report-imported"
              >{{ report.imported }}</span
            >
          </SettingsRow>
          <SettingsRow :title="t('passwords.import.report.trashed')">
            <span class="tabular-nums">{{ report.trashed }}</span>
          </SettingsRow>
          <SettingsRow :title="t('passwords.import.report.historyStates')">
            <span class="tabular-nums">{{ report.historyStates }}</span>
          </SettingsRow>
          <SettingsRow :title="t('passwords.import.report.skippedDuplicates')">
            <span class="tabular-nums">{{ report.skippedDuplicates }}</span>
          </SettingsRow>
          <SettingsRow
            v-if="report.referencesConverted + report.referencesLeftAsText > 0"
            :title="t('passwords.import.report.referencesConverted')"
            :description="
              report.referencesLeftAsText > 0
                ? t('passwords.import.report.referencesLeftAsText', {
                    count: report.referencesLeftAsText,
                  })
                : undefined
            "
            data-testid="passwords-import-references"
          >
            <span class="tabular-nums">{{ report.referencesConverted }}</span>
          </SettingsRow>
        </SettingsGroup>

        <template v-if="groups.length">
          <h2 class="px-1 text-sm font-semibold">
            {{ t('passwords.import.needsAttention') }}
          </h2>
          <ul
            class="flex flex-col gap-2"
            data-testid="passwords-import-attention"
          >
            <li
              v-for="group in groups"
              :key="group.key"
              class="rounded-xl bg-muted px-4 py-3"
            >
              <div class="flex flex-wrap items-center gap-2">
                <div class="flex min-w-0 flex-1 flex-col">
                  <span class="truncate font-medium">
                    {{
                      group.key === 'source'
                        ? t('passwords.import.sourceGroup')
                        : group.title || t('passwords.untitled')
                    }}
                  </span>
                  <span
                    v-if="group.folderPath"
                    class="truncate text-sm text-muted-foreground"
                  >
                    {{ group.folderPath }}
                  </span>
                </div>
                <UiButton
                  v-if="group.itemId"
                  variant="outline"
                  size="sm"
                  @click="router.push(`/entry/${group.itemId}`)"
                >
                  {{ t('passwords.import.openEntry') }}
                </UiButton>
              </div>
              <ul class="mt-2 list-disc pl-5 text-sm">
                <li v-for="(row, index) in group.rows" :key="index">
                  {{ rowText(row) }}
                </li>
              </ul>
            </li>
          </ul>
        </template>
        <p v-else class="text-sm text-muted-foreground">
          {{ t('passwords.import.nothingToDo') }}
        </p>

        <div class="flex flex-wrap justify-end gap-2">
          <UiButton
            variant="outline"
            data-testid="passwords-import-save-report"
            @click="saveReportAsync"
          >
            <Icon name="lucide:file-down" class="size-4" />
            {{ t('passwords.import.saveReport') }}
          </UiButton>
          <UiButton variant="outline" @click="again">{{
            t('passwords.import.another')
          }}</UiButton>
          <UiButton @click="router.push('/')">{{
            t('passwords.import.toList')
          }}</UiButton>
        </div>
      </template>
    </div>
  </div>
</template>
