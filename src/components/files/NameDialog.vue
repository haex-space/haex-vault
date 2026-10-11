<script setup lang="ts">
/**
 * A new folder, or a new name for an entry (spec 044 FR-017), or a new encrypted folder (spec 048
 * FR-001): a name that is taken or not allowed is refused with the reason, and the dialog stays open.
 */
import type { Entry } from '@bindings/Entry'
import type { SourceRef } from '@bindings/SourceRef'
import { asFilesError } from '~/composables/useFiles'

const props = defineProps<{
  source: SourceRef
  /** The open folder (a new folder goes in here). */
  folder: string
  /** The entry to rename, or `null` for a new folder. */
  entry: Entry | null
  /** A new folder is an encrypted folder (spec 048 FR-001). */
  encrypted?: boolean
}>()

const emit = defineEmits<{ done: [entry: Entry] }>()
const open = defineModel<boolean>('open', { required: true })

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const { createFolderAsync, createEncryptedFolderAsync, renameAsync } =
  useFiles()

const name = ref('')
const error = ref<string | null>(null)
const saving = ref(false)

watch(open, (isOpen) => {
  if (!isOpen) return
  name.value = props.entry?.name ?? ''
  error.value = null
})

async function saveAsync() {
  saving.value = true
  try {
    const entry = props.entry
      ? await renameAsync(props.source, props.entry.path, name.value)
      : props.encrypted
        ? await createEncryptedFolderAsync(
            props.source,
            props.folder,
            name.value,
          )
        : await createFolderAsync(props.source, props.folder, name.value)
    open.value = false
    emit('done', entry)
  } catch (cause) {
    const code = asFilesError(cause)?.code
    error.value = code ? t(`files.error.${code}`) : String(cause)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <UiDrawerModal
    v-model:open="open"
    :title="
      entry
        ? t('files.name.rename')
        : encrypted
          ? t('files.name.newEncryptedFolder')
          : t('files.name.newFolder')
    "
  >
    <template #content>
      <form
        class="flex flex-col gap-4"
        data-testid="files-name-form"
        @submit.prevent="saveAsync"
      >
        <UiInput
          id="files-name"
          v-model="name"
          :label="t('files.name.label')"
          :labels="fieldLabels.input.value"
          data-testid="files-name-input"
        />
        <p
          v-if="error"
          class="text-sm text-destructive"
          role="alert"
          data-testid="files-name-error"
        >
          {{ error }}
        </p>
        <div class="flex justify-end gap-2">
          <UiButton type="button" variant="outline" @click="open = false">{{
            t('files.cancel')
          }}</UiButton>
          <UiButton
            type="submit"
            :loading="saving"
            data-testid="files-name-save"
            >{{
              entry ? t('files.name.renameAction') : t('files.name.create')
            }}</UiButton
          >
        </div>
      </form>
    </template>
  </UiDrawerModal>
</template>
