<template>
  <UiDrawerModal
    v-model:open="open"
    :title="title"
    :description="description"
  >
    <template #body>
      <div class="space-y-4">
        <div class="space-y-2">
          <p class="text-sm font-medium">
            {{ fileLabel }}
          </p>
          <input
            ref="fileInput"
            type="file"
            :accept="accept"
            class="hidden"
            @change="onFileChange"
          >
          <UButton
            icon="i-lucide-file"
            variant="outline"
            color="neutral"
            class="w-full justify-start"
            @click="fileInput?.click()"
          >
            {{ selectedFileName || t('chooseFile') }}
          </UButton>
          <p
            v-if="fileHint"
            class="text-xs text-muted"
          >
            {{ fileHint }}
          </p>
        </div>

        <!-- Provider-specific extra inputs (e.g. KeePass master password) -->
        <slot
          name="extra"
          :selected-file="selectedFile"
        />

        <div
          v-if="importing"
          class="space-y-2"
        >
          <UProgress :value="progress" />
          <p class="text-sm text-center text-muted">
            {{ t('importing') }}: {{ progress }}%
          </p>
        </div>

        <div
          v-if="error"
          class="p-4 bg-error/10 text-error rounded-lg text-sm"
        >
          {{ error }}
        </div>
      </div>
    </template>

    <template #footer>
      <div class="flex gap-2 justify-end">
        <UButton
          color="neutral"
          variant="outline"
          @click="() => { open = false }"
        >
          {{ t('cancel') }}
        </UButton>
        <UButton
          :disabled="!canImport"
          :loading="importing"
          @click="runImport"
        >
          {{ t('import') }}
        </UButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts" generic="TStats extends Record<string, unknown>">
import { useToast } from '#imports'

const props = defineProps<{
  title: string
  description: string
  fileLabel: string
  fileHint?: string
  accept: string
  /**
   * Optional gate for provider-specific extra inputs (e.g. KeePass needs a
   * master password before import becomes available). Defaults to `true`.
   */
  extraValid?: boolean
  successTitle: string
  successDescription: (stats: TStats) => string
  errorImportLabel: string
  errorNoFileLabel: string
  /**
   * Provider-specific import routine. Called with the user-selected file and a
   * progress callback that takes a 0–100 percentage. Throws on failure; the
   * shell catches and surfaces the error inline (no toast for errors so the
   * user can see what went wrong before they close the dialog).
   */
  doImport: (
    file: File,
    setProgress: (pct: number) => void,
  ) => Promise<TStats>
}>()

const open = defineModel<boolean>('open', { default: false })
const { t } = useI18n()
const toast = useToast()

const fileInput = useTemplateRef<HTMLInputElement>('fileInput')
const selectedFile = ref<File | null>(null)
const selectedFileName = ref<string | null>(null)
const importing = ref(false)
const progress = ref(0)
const error = ref<string | null>(null)

// extraValid defaults to true when the caller doesn't pass it (i.e. no extra
// inputs to gate on — the file alone is enough).
const canImport = computed(() =>
  !!selectedFile.value
  && !importing.value
  && (props.extraValid === undefined || props.extraValid),
)

function onFileChange(event: Event) {
  const target = event.target as HTMLInputElement
  const file = target.files?.[0] ?? null
  selectedFile.value = file
  selectedFileName.value = file?.name ?? null
  error.value = null
}

async function runImport() {
  if (!selectedFile.value) {
    error.value = props.errorNoFileLabel
    return
  }
  importing.value = true
  progress.value = 0
  error.value = null
  try {
    const stats = await props.doImport(selectedFile.value, (pct) => {
      progress.value = pct
    })
    toast.add({
      title: props.successTitle,
      description: props.successDescription(stats),
      color: 'success',
    })
    open.value = false
    reset()
  }
  catch (err) {
    console.error('[ImportWizardShell]', err)
    error.value
      = props.errorImportLabel
        + ': '
        + (err instanceof Error ? err.message : String(err))
  }
  finally {
    importing.value = false
    progress.value = 0
  }
}

function reset() {
  selectedFile.value = null
  selectedFileName.value = null
  error.value = null
  importing.value = false
  progress.value = 0
}

watch(open, (v) => {
  if (!v) reset()
})
</script>

<i18n lang="yaml">
de:
  chooseFile: Datei auswählen
  cancel: Abbrechen
  import: Importieren
  importing: Importiere

en:
  chooseFile: Choose file
  cancel: Cancel
  import: Import
  importing: Importing
</i18n>
