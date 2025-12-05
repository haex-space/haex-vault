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
          {{ t('title') }}
        </h3>
        <UButton
          icon="i-heroicons-x-mark"
          color="neutral"
          variant="ghost"
          @click="onAbort"
        />
      </div>
    </template>

    <template #content>
      <div class="flex flex-col gap-4">
        <i18n-t
          keypath="question"
          tag="p"
        >
          <template #name>
            <span class="font-bold text-primary">{{ extension?.name }}</span>
          </template>
        </i18n-t>

        <div
          v-if="extension"
          class="bg-gray-100 dark:bg-gray-800 rounded-lg p-4"
        >
          <div class="flex items-center gap-3">
            <UIcon
              v-if="extension.icon"
              :name="extension.icon"
              class="w-12 h-12"
            />
            <UIcon
              v-else
              name="i-heroicons-puzzle-piece"
              class="w-12 h-12 text-gray-400"
            />
            <div class="flex-1">
              <h4 class="font-semibold">
                {{ extension.name }}
              </h4>
              <p class="text-sm text-gray-500 dark:text-gray-400">
                {{ t('version') }}: {{ extension.version }}
              </p>
              <p
                v-if="extension.author"
                class="text-sm text-gray-500 dark:text-gray-400"
              >
                {{ t('author') }}: {{ extension.author }}
              </p>
            </div>
          </div>
        </div>

        <!-- Delete Mode Selection -->
        <URadioGroup
          v-model="deleteMode"
          :items="deleteModeItems"
        />

        <!-- Warning for complete deletion -->
        <UAlert
          v-if="deleteMode === 'complete'"
          color="error"
          variant="soft"
          :title="t('warning.title')"
          :description="t('warning.description')"
          icon="i-heroicons-exclamation-triangle"
        />

        <!-- Info for device-only removal -->
        <UAlert
          v-else
          color="info"
          variant="soft"
          :title="t('info.title')"
          :description="t('info.description')"
          icon="i-heroicons-information-circle"
        />
      </div>
    </template>

    <template #footer>
      <div class="flex flex-col sm:flex-row gap-4 justify-end w-full">
        <UButton
          icon="i-heroicons-x-mark"
          :label="t('abort')"
          color="neutral"
          variant="outline"
          class="w-full sm:w-auto"
          @click="onAbort"
        />
        <UButton
          icon="i-heroicons-trash"
          :label="t('confirm')"
          color="error"
          class="w-full sm:w-auto"
          @click="onConfirm"
        />
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import type { IHaexSpaceExtension } from '~/types/haexspace'

export type DeleteMode = 'device' | 'complete'

const emit = defineEmits<{
  confirm: [deleteMode: DeleteMode]
  abort: []
}>()

const { t } = useI18n()

defineProps<{ extension?: IHaexSpaceExtension }>()

const open = defineModel<boolean>('open')
const deleteMode = ref<DeleteMode>('device')

const deleteModeItems = computed(() => [
  {
    value: 'device',
    label: t('mode.device.label'),
    description: t('mode.device.description'),
  },
  {
    value: 'complete',
    label: t('mode.complete.label'),
    description: t('mode.complete.description'),
  },
])

// Reset to default when dialog opens
watch(open, (isOpen) => {
  if (isOpen) {
    deleteMode.value = 'device'
  }
})

const onAbort = () => {
  open.value = false
  emit('abort')
}

const onConfirm = () => {
  open.value = false
  emit('confirm', deleteMode.value)
}
</script>

<i18n lang="yaml">
de:
  title: Erweiterung entfernen
  question: Möchtest du {name} wirklich entfernen?
  mode:
    device:
      label: Nur von diesem Gerät entfernen
      description: Die Erweiterung wird deinstalliert, aber alle Daten bleiben erhalten und werden weiter synchronisiert.
    complete:
      label: Komplett löschen
      description: Die Erweiterung und alle zugehörigen Daten werden dauerhaft gelöscht.
  warning:
    title: Achtung
    description: Diese Aktion kann nicht rückgängig gemacht werden. Alle Daten der Erweiterung werden dauerhaft gelöscht.
  info:
    title: Hinweis
    description: Die Daten der Erweiterung bleiben erhalten und werden weiter synchronisiert. Du kannst die Erweiterung jederzeit wieder installieren.
  version: Version
  author: Autor
  abort: Abbrechen
  confirm: Entfernen

en:
  title: Remove Extension
  question: Do you really want to remove {name}?
  mode:
    device:
      label: Remove from this device only
      description: The extension will be uninstalled, but all data will be preserved and continue to sync.
    complete:
      label: Delete completely
      description: The extension and all associated data will be permanently deleted.
  warning:
    title: Warning
    description: This action cannot be undone. All extension data will be permanently deleted.
  info:
    title: Note
    description: The extension data will be preserved and continue to sync. You can reinstall the extension at any time.
  version: Version
  author: Author
  abort: Cancel
  confirm: Remove
</i18n>
