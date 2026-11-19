<script setup lang="ts">
/**
 * "Importieren" and "Exportieren" of the appearance (spec 035-appearance-and-fields, FR-021,
 * contracts/appearance-file.md): the file dialog and the file access happen here, the checking and
 * the writing in the actions `settings.appearance.export` and `.import`. A file is read only after
 * the user has chosen it, and never larger than 16 KiB. The file plugin opens a chosen path and,
 * on Android, a provider's `content://` address alike (spec 043).
 */
import { open, save } from '@tauri-apps/plugin-dialog'
import { readTextFile, stat, writeTextFile } from '@tauri-apps/plugin-fs'
import { FILE_MAX_BYTES } from '~/lib/appearance/schema'

const emit = defineEmits<{
  done: [message: string]
  failed: [message: string]
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const exportAction = useActionOrThrow('settings.appearance.export')
const importAction = useActionOrThrow('settings.appearance.import')
const busy = ref(false)

const FILTERS = [{ name: 'holzi appearance', extensions: ['json'] }]
const { readyAsync } = useDeviceCapabilities()

/** Android's document providers do not report extensions reliably (spec 043): no filter there. */
async function filtersAsync() {
  return (await readyAsync())?.platform === 'android' ? undefined : FILTERS
}

function tooLarge() {
  return {
    kind: 'AppearanceError',
    reason: 'settings.appearance.import.notJson',
  }
}

async function exportAsync() {
  busy.value = true
  try {
    const path = await save({
      defaultPath: 'holzi.haex-appearance.json',
      filters: await filtersAsync(),
    })
    if (!path) return
    const { file } = (await exportAction()) as { file: string }
    await writeTextFile(path, file)
    emit('done', t('settings.appearance.exported'))
  } catch (error: unknown) {
    emit('failed', errString(error))
  } finally {
    busy.value = false
  }
}

async function importAsync() {
  busy.value = true
  try {
    const path = await open({ multiple: false, filters: await filtersAsync() })
    if (typeof path !== 'string') return
    // A provider address (Android, spec 043) may have no size; the text is checked after reading.
    const size = await stat(path).then(
      (info) => info.size,
      () => null,
    )
    if (size !== null && size > FILE_MAX_BYTES) throw tooLarge()
    const file = await readTextFile(path)
    if (new TextEncoder().encode(file).length > FILE_MAX_BYTES) throw tooLarge()
    await importAction({ file })
    emit('done', t('settings.appearance.imported'))
  } catch (error: unknown) {
    emit('failed', errString(error))
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="flex flex-wrap items-center gap-2">
    <UiButton
      variant="outline"
      size="sm"
      :disabled="busy"
      data-testid="appearance-import"
      @click="importAsync"
    >
      {{ t('settings.appearance.importFile') }}
    </UiButton>
    <UiButton
      variant="outline"
      size="sm"
      :disabled="busy"
      data-testid="appearance-export"
      @click="exportAsync"
    >
      {{ t('settings.appearance.exportFile') }}
    </UiButton>
  </div>
</template>
