<template>
  <UiDialogConfirm
    v-model:open="open"
    @abort="onDeny"
    @confirm="onConfirm"
  >
    <template #title>
      {{ mode === 'update' ? t('update.title', { extensionName: preview?.manifest.name }) : t('reinstall.title', { extensionName: preview?.manifest.name }) }}
    </template>

    <template #body>
      <div class="flex flex-col gap-4">
        <p>
          {{
            mode === 'update'
              ? t('update.question', { extensionName: preview?.manifest.name })
              : t('reinstall.question', { extensionName: preview?.manifest.name })
          }}
        </p>

        <UAlert
          v-if="mode === 'update'"
          color="info"
          variant="soft"
          :title="t('update.info.title')"
          :description="t('update.info.description')"
          icon="i-heroicons-information-circle"
        />
        <UAlert
          v-else
          color="error"
          variant="soft"
          :title="t('reinstall.warning.title')"
          :description="t('reinstall.warning.description')"
          icon="i-heroicons-exclamation-triangle"
        />

        <div
          v-if="preview"
          class="bg-gray-100 dark:bg-gray-800 rounded-lg p-4"
        >
          <div class="flex items-center gap-3">
            <UIcon
              v-if="preview.manifest.icon"
              :name="preview.manifest.icon"
              class="w-12 h-12"
            />
            <div class="flex-1">
              <h4 class="font-semibold">
                {{ preview.manifest.name }}
              </h4>
              <p class="text-sm text-gray-500 dark:text-gray-400">
                {{ t('version') }}: {{ preview.manifest.version }}
              </p>
            </div>
          </div>
        </div>
      </div>
    </template>
  </UiDialogConfirm>
</template>

<script setup lang="ts">
import type { ExtensionPreview } from '~~/src-tauri/bindings/ExtensionPreview'

export type ReinstallMode = 'update' | 'reinstall'

const { t } = useI18n()

const open = defineModel<boolean>('open', { default: false })
const preview = defineModel<ExtensionPreview | null>('preview', {
  default: null,
})

withDefaults(defineProps<{
  mode?: ReinstallMode
}>(), {
  mode: 'reinstall',
})

const emit = defineEmits(['deny', 'confirm'])

const onDeny = () => {
  open.value = false
  emit('deny')
}

const onConfirm = () => {
  open.value = false
  emit('confirm')
}
</script>

<i18n lang="yaml">
de:
  update:
    title: '{extensionName} aktualisieren'
    question: Möchtest du {extensionName} auf die neueste Version aktualisieren?
    info:
      title: Hinweis
      description: Deine Daten bleiben erhalten. Nur die Erweiterungsdateien werden aktualisiert.
  reinstall:
    title: '{extensionName} neu installieren'
    question: Soll die Erweiterung {extensionName} komplett neu installiert werden?
    warning:
      title: Achtung
      description: Alle Daten der Erweiterung werden gelöscht und die Erweiterung wird neu installiert. Diese Aktion kann nicht rückgängig gemacht werden.
  version: Version

en:
  update:
    title: 'Update {extensionName}'
    question: Do you want to update {extensionName} to the latest version?
    info:
      title: Note
      description: Your data will be preserved. Only the extension files will be updated.
  reinstall:
    title: 'Reinstall {extensionName}'
    question: Do you want to completely reinstall {extensionName}?
    warning:
      title: Warning
      description: All extension data will be deleted and the extension will be reinstalled. This action cannot be undone.
  version: Version
</i18n>