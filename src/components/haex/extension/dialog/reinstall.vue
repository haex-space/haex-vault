<template>
  <UiDrawerModal
    v-model:open="open"
    :ui="{
      content: 'sm:max-w-xl sm:mx-auto',
    }"
  >
    <template #header>
      <div class="flex items-center justify-between w-full">
        <h3 class="text-lg font-semibold">
          {{ mode === 'update' ? t('update.title', { extensionName: preview?.manifest.name }) : t('reinstall.title', { extensionName: preview?.manifest.name }) }}
        </h3>
        <UButton
          icon="i-heroicons-x-mark"
          color="neutral"
          variant="ghost"
          @click="onDeny"
        />
      </div>
    </template>

    <template #content>
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
            <div class="w-12 h-12 shrink-0 rounded-lg bg-base-200 flex items-center justify-center overflow-hidden">
              <HaexIcon
                :name="iconUrl || preview.manifest.icon || 'i-heroicons-puzzle-piece'"
                class="w-full h-full object-contain"
              />
            </div>
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

    <template #footer>
      <div class="flex flex-col sm:flex-row gap-4 justify-end w-full">
        <UiButton
          icon="i-heroicons-x-mark"
          :label="t('abort')"
          color="neutral"
          variant="outline"
          class="w-full sm:w-auto"
          @click="onDeny"
        />
        <UiButton
          :icon="mode === 'update' ? 'i-heroicons-arrow-path' : 'i-heroicons-arrow-down-tray'"
          :label="mode === 'update' ? t('update.confirm') : t('reinstall.confirm')"
          :color="mode === 'update' ? 'primary' : 'error'"
          class="w-full sm:w-auto"
          @click="onConfirm"
        />
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import type { ExtensionPreview } from '~~/src-tauri/bindings/ExtensionPreview'

export type ReinstallMode = 'update' | 'reinstall'

const { t } = useI18n()

const open = defineModel<boolean>('open', { default: false })
const preview = defineModel<ExtensionPreview | null>('preview', {
  default: null,
})

const props = withDefaults(defineProps<{
  mode?: ReinstallMode
  /** Icon URL from marketplace (optional - used when manifest icon is not available) */
  iconUrl?: string | null
}>(), {
  mode: 'reinstall',
  iconUrl: null,
})

const { iconUrl } = toRefs(props)

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
    confirm: Aktualisieren
  reinstall:
    title: '{extensionName} neu installieren'
    question: Soll die Erweiterung {extensionName} komplett neu installiert werden?
    warning:
      title: Achtung
      description: Alle Daten der Erweiterung werden gelöscht und die Erweiterung wird neu installiert. Diese Aktion kann nicht rückgängig gemacht werden.
    confirm: Neu installieren
  version: Version
  abort: Abbrechen

en:
  update:
    title: 'Update {extensionName}'
    question: Do you want to update {extensionName} to the latest version?
    info:
      title: Note
      description: Your data will be preserved. Only the extension files will be updated.
    confirm: Update
  reinstall:
    title: 'Reinstall {extensionName}'
    question: Do you want to completely reinstall {extensionName}?
    warning:
      title: Warning
      description: All extension data will be deleted and the extension will be reinstalled. This action cannot be undone.
    confirm: Reinstall
  version: Version
  abort: Cancel
</i18n>