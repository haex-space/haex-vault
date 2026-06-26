<template>
  <div
    class="flex-none flex items-center gap-2 px-3 py-2 bg-elevated/50 backdrop-blur-md border-b border-default"
  >
    <UiButton
      :tooltip="t('back')"
      icon="i-lucide-arrow-left"
      color="neutral"
      variant="ghost"
      type="button"
      class="shrink-0"
      @click="emit('back')"
    />

    <div class="flex items-center gap-2 min-w-0 flex-1">
      <div
        v-if="!isCreating"
        class="shrink-0 size-8 rounded-md flex items-center justify-center bg-elevated overflow-hidden"
        :style="iconBackgroundStyle"
      >
        <UIcon
          v-if="iconDescriptor.kind === 'iconify'"
          :name="iconDescriptor.name"
          class="size-5"
          :class="color ? '' : 'text-primary'"
        />
        <img
          v-else-if="binaryIconSrc"
          :src="binaryIconSrc"
          :alt="title || 'icon'"
          class="size-6 object-contain"
        >
        <UIcon
          v-else
          name="i-lucide-key"
          class="size-5 text-muted"
        />
      </div>
      <h2 class="font-semibold truncate">
        {{
          isCreating
            ? t('titleCreate')
            : title || t('untitled')
        }}
      </h2>
    </div>

    <div class="flex items-center gap-1 shrink-0">
      <template v-if="isEditing">
        <UiButton
          :label="t('save')"
          icon="i-lucide-save"
          color="primary"
          type="submit"
          :loading="saving"
        />
      </template>
      <template v-else>
        <UiButton
          v-if="isCurrentItemInTrash"
          :tooltip="t('restore')"
          icon="i-lucide-undo-2"
          color="neutral"
          variant="ghost"
          type="button"
          @click="emit('restore')"
        />
        <UiButton
          v-if="!isCurrentItemInTrash"
          :tooltip="t('edit')"
          icon="i-lucide-pencil"
          color="neutral"
          variant="ghost"
          type="button"
          @click="emit('startEdit')"
        />
        <UiButton
          :tooltip="isCurrentItemInTrash ? t('deletePermanently') : t('delete')"
          :icon="isCurrentItemInTrash ? 'i-lucide-trash-2' : 'i-lucide-trash'"
          color="error"
          variant="ghost"
          type="button"
          @click="emit('requestDelete')"
        />
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
const { t } = useI18n()

defineProps<{
  title: string
  color: string
  isCreating: boolean
  isEditing: boolean
  isCurrentItemInTrash: boolean
  saving: boolean
  iconDescriptor: { kind: 'iconify'; name: string } | { kind: 'binary'; hash: string }
  binaryIconSrc: string | null
  iconBackgroundStyle: { backgroundColor: string } | undefined
}>()

const emit = defineEmits<{
  back: []
  restore: []
  startEdit: []
  requestDelete: []
}>()
</script>

<i18n lang="yaml">
de:
  titleCreate: Neuer Eintrag
  untitled: (ohne Titel)
  back: Zurück
  edit: Bearbeiten
  delete: In Papierkorb
  deletePermanently: Endgültig löschen
  restore: Wiederherstellen
  save: Speichern
en:
  titleCreate: New entry
  untitled: (untitled)
  back: Back
  edit: Edit
  delete: Move to trash
  deletePermanently: Delete permanently
  restore: Restore
  save: Save
</i18n>
