<script setup lang="ts">
/**
 * "Eigenes Icon-Set hinzufügen" (spec 053 US3, FR-002, FR-003, FR-018): a file (an archive or a
 * VSIX) or, where the device can read folders, a folder. Step 1 asks Rust what the import would
 * do and shows it; nothing is written. Step 2 imports; Rust reads the source again and its check
 * wins. The window sees a path or an address only to hand it on, never the bytes.
 */
import { invoke } from '@tauri-apps/api/core'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import type { IconImportReport } from '@bindings/IconImportReport'
import type { IconSetInfo } from '@bindings/IconSetInfo'
import type { IconSource } from '@bindings/IconSource'
import { formatFileSize } from '~/lib/passwords/format'

const emit = defineEmits<{ imported: [info: IconSetInfo] }>()

const { t } = useI18n()
const { errString } = useErrorString()
const { capabilities } = useDeviceCapabilities()
const { pickOneAsync } = usePickedFile()

const open = ref(false)
const source = ref<IconSource | null>(null)
const report = ref<IconImportReport | null>(null)
const replace = ref(false)
const busy = ref(false)
const failure = ref<string | null>(null)

/** A name that is taken needs the user's word before the old set goes. */
const blocked = computed(
  () =>
    busy.value ||
    report.value === null ||
    (report.value.sameName !== null && !replace.value),
)

function reset() {
  source.value = null
  report.value = null
  replace.value = false
  failure.value = null
}

async function startAsync(next: IconSource) {
  reset()
  source.value = next
  open.value = true
  busy.value = true
  try {
    report.value = await invoke<IconImportReport>(
      'file_icon_set_import_preview',
      { source: next },
    )
  } catch (error) {
    failure.value = errString(error)
  } finally {
    busy.value = false
  }
}

async function chooseFileAsync() {
  const file = await pickOneAsync([
    {
      name: t('settings.iconSet.import.fileType'),
      extensions: ['zip', 'vsix'],
    },
  ])
  if (file) await startAsync({ kind: 'file', file })
}

async function chooseFolderAsync() {
  const path = await openDialog({ multiple: false, directory: true })
  if (typeof path === 'string') await startAsync({ kind: 'folder', path })
}

async function importAsync() {
  if (!source.value || !report.value || blocked.value) return
  busy.value = true
  failure.value = null
  try {
    const added = await invoke<IconSetInfo>('file_icon_set_import', {
      source: source.value,
      replace: replace.value ? report.value.sameName : null,
    })
    open.value = false
    emit('imported', added)
  } catch (error) {
    failure.value = errString(error)
  } finally {
    busy.value = false
  }
}

watch(open, (isOpen) => {
  if (!isOpen) reset()
})

defineExpose({ startAsync })
</script>

<template>
  <SettingsGroup>
    <SettingsRow
      navigates
      icon="lucide:file-plus"
      :title="t('settings.iconSet.import.chooseFile')"
      :description="t('settings.iconSet.import.chooseFileHint')"
      data-testid="settings-icon-set-import"
      @select="chooseFileAsync"
    />
    <SettingsRow
      v-if="capabilities?.folderPick !== false"
      navigates
      icon="lucide:folder-plus"
      :title="t('settings.iconSet.import.chooseFolder')"
      data-testid="settings-icon-set-import-folder"
      @select="chooseFolderAsync"
    />
  </SettingsGroup>

  <UiDrawerModal
    v-model:open="open"
    :title="t('settings.iconSet.import.title')"
  >
    <template #content>
      <div class="flex flex-col gap-4" data-testid="settings-icon-set-report">
        <p v-if="busy && !report" class="text-sm text-muted-foreground">
          {{ t('settings.iconSet.import.checking') }}
        </p>

        <template v-if="report">
          <div class="rounded-xl bg-muted px-4 py-3 text-sm">
            <p class="text-base font-semibold">{{ report.name }}</p>
            <p
              v-if="report.author || report.license"
              class="text-muted-foreground"
            >
              {{ [report.author, report.license].filter(Boolean).join(' · ') }}
            </p>
            <p class="mt-1">
              {{
                t('settings.iconSet.import.summary', {
                  count: report.imageCount,
                  size: formatFileSize(Number(report.bytes)),
                })
              }}
            </p>
          </div>

          <p class="rounded-xl bg-warning/15 px-4 py-3 text-sm">
            {{ t('settings.iconSet.import.licenseNotice') }}
          </p>

          <div v-if="report.skipped.length > 0" class="flex flex-col gap-1">
            <h3 class="text-sm font-semibold">
              {{
                t('settings.iconSet.import.skipped', {
                  count: report.skipped.length,
                })
              }}
            </h3>
            <ul
              class="max-h-40 overflow-y-auto rounded-xl bg-muted px-4 py-2 text-xs"
              data-testid="settings-icon-set-skipped"
            >
              <li
                v-for="item in report.skipped"
                :key="item.path"
                class="py-0.5"
              >
                <span class="font-mono">{{ item.path }}</span>
                <span class="text-muted-foreground"> · {{ item.reason }}</span>
              </li>
            </ul>
          </div>

          <label
            v-if="report.sameName"
            class="flex items-center gap-2 rounded-xl bg-muted px-4 py-3 text-sm"
          >
            <ShadcnCheckbox
              v-model="replace"
              data-testid="settings-icon-set-replace"
            />
            {{ t('settings.iconSet.import.replace') }}
          </label>
        </template>

        <p v-if="failure" class="text-sm text-destructive" role="alert">
          {{ failure }}
        </p>

        <div class="flex justify-end gap-2">
          <UiButton variant="outline" @click="open = false">
            {{ t('settings.iconSet.import.cancel') }}
          </UiButton>
          <UiButton
            v-if="report"
            :disabled="blocked"
            :loading="busy"
            data-testid="settings-icon-set-import-confirm"
            @click="importAsync"
          >
            {{ t('settings.iconSet.import.confirm') }}
          </UiButton>
        </div>
      </div>
    </template>
  </UiDrawerModal>
</template>
