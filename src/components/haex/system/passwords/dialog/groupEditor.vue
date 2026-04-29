<template>
  <UModal
    v-model:open="open"
    :title="isEditing ? t('titleEdit') : t('titleCreate')"
    :dismissible="true"
  >
    <template #body>
      <form
        class="flex flex-col gap-4"
        @submit.prevent="onSubmit"
      >
        <UiInput
          v-model="form.name"
          v-model:errors="nameErrors"
          :label="t('fields.name')"
          :placeholder="t('fields.namePlaceholder')"
          required
        />

        <UiTextarea
          v-model="form.description"
          :label="t('fields.description')"
          :placeholder="t('fields.descriptionPlaceholder')"
          :rows="3"
        />

        <div class="flex items-end gap-3">
          <div class="flex-1 min-w-0">
            <label class="text-xs font-medium text-highlighted mb-1 block">
              {{ t('fields.icon') }}
            </label>
            <HaexSystemPasswordsEditorIconPicker
              v-model="form.icon"
              :color="form.color || undefined"
            />
          </div>
          <div class="flex flex-col gap-1">
            <label class="text-xs font-medium text-highlighted">
              {{ t('fields.color') }}
            </label>
            <input
              v-model="form.color"
              type="color"
              class="size-10 rounded-md border border-default cursor-pointer p-0 bg-transparent"
            >
          </div>
        </div>

        <button
          type="submit"
          class="hidden"
          aria-hidden="true"
        />
      </form>
    </template>
    <template #footer>
      <div class="flex flex-col sm:flex-row gap-2 justify-end w-full">
        <UiButton
          icon="i-lucide-x"
          :label="t('cancel')"
          color="neutral"
          variant="outline"
          @click="open = false"
        />
        <UiButton
          icon="i-lucide-save"
          :label="t('save')"
          color="primary"
          :loading="saving"
          @click="onSubmit"
        />
      </div>
    </template>
  </UModal>
</template>

<script setup lang="ts">
import type { SelectHaexPasswordsGroups } from '~/database/schemas'

const props = defineProps<{
  mode: 'create' | 'edit'
  group: SelectHaexPasswordsGroups | null
  createParentId?: string | null
}>()

const open = defineModel<boolean>('open', { default: false })
const { t } = useI18n()
const toast = useToast()

const groupsStore = usePasswordsGroupsStore()

const form = reactive({
  name: '',
  description: '',
  icon: '',
  color: '',
})

const nameErrors = ref<string[]>([])
const saving = ref(false)

const isEditing = computed(() => props.mode === 'edit' && !!props.group?.id)

watch(
  () => [open.value, props.group, props.mode] as const,
  ([isOpen, group]) => {
    if (!isOpen) return
    form.name = group?.name ?? ''
    form.description = group?.description ?? ''
    form.icon = group?.icon ?? ''
    form.color = group?.color ?? ''
    nameErrors.value = []
  },
  { immediate: true },
)

const onSubmit = async () => {
  if (saving.value) return
  nameErrors.value = []

  const trimmed = form.name.trim()
  if (!trimmed) {
    nameErrors.value = [t('validation.nameRequired')]
    return
  }

  saving.value = true
  try {
    if (isEditing.value && props.group) {
      await groupsStore.updateGroupAsync({
        ...props.group,
        name: trimmed,
        description: form.description || null,
        icon: form.icon || null,
        color: form.color || null,
      })
      toast.add({ title: t('toast.updated'), color: 'success' })
    } else {
      const parentId = props.createParentId ?? null
      await groupsStore.addGroupAsync({
        name: trimmed,
        description: form.description || null,
        icon: form.icon || null,
        color: form.color || null,
        parentId,
      })
      toast.add({ title: t('toast.created'), color: 'success' })
    }
    open.value = false
  } catch (error) {
    console.error('[GroupEditor] save failed:', error)
    toast.add({
      title: t('toast.saveError'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
      icon: 'i-lucide-alert-triangle',
    })
  } finally {
    saving.value = false
  }
}
</script>

<i18n lang="yaml">
de:
  titleCreate: Ordner anlegen
  titleEdit: Ordner bearbeiten
  fields:
    name: Name
    namePlaceholder: z.B. Arbeit
    description: Beschreibung
    descriptionPlaceholder: Optional
    icon: Icon
    color: Farbe
  validation:
    nameRequired: Name ist Pflicht
  cancel: Abbrechen
  save: Speichern
  toast:
    created: Ordner erstellt
    updated: Ordner aktualisiert
    saveError: Speichern fehlgeschlagen
en:
  titleCreate: Create folder
  titleEdit: Edit folder
  fields:
    name: Name
    namePlaceholder: e.g. Work
    description: Description
    descriptionPlaceholder: Optional
    icon: Icon
    color: Color
  validation:
    nameRequired: Name is required
  cancel: Cancel
  save: Save
  toast:
    created: Folder created
    updated: Folder updated
    saveError: Save failed
</i18n>
