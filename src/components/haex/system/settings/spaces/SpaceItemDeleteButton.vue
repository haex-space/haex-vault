<template>
  <UiButton
    color="error"
    variant="ghost"
    size="xs"
    icon="i-lucide-trash-2"
    :title="t('actions.delete')"
    :data-testid="`space-item-delete-${itemType}-${itemId}`"
    @click.stop="showConfirm = true"
  />

  <UiDialogConfirm
    v-model:open="showConfirm"
    :title="t('confirm.title')"
    :description="confirmDescription"
    :confirm-label="t('actions.confirm')"
    :abort-label="t('actions.cancel')"
    :confirm-disabled="isDeleting"
    confirm-icon="i-lucide-trash-2"
    @confirm="onConfirmAsync"
  />
</template>

<script setup lang="ts">
import {
  useSpaceItemDelete,
  SpaceItemNotSupportedError,
  type SpaceItemType,
} from '@/composables/useSpaceItemDelete'

const props = defineProps<{
  itemType: SpaceItemType
  itemId: string
  spaceId: string
  label?: string
}>()

const emit = defineEmits<{
  deleted: []
}>()

const { t } = useI18n()
const { add: addToast } = useToast()
const { deleteItem } = useSpaceItemDelete()

const showConfirm = ref(false)
const isDeleting = ref(false)

const confirmDescription = computed(() =>
  t('confirm.body', { label: props.label ?? '' }),
)

const onConfirmAsync = async () => {
  if (isDeleting.value) return
  isDeleting.value = true
  try {
    await deleteItem({
      itemType: props.itemType,
      itemId: props.itemId,
      spaceId: props.spaceId,
      label: props.label,
    })
    showConfirm.value = false
    addToast({ title: t('toasts.success'), color: 'success' })
    emit('deleted')
  } catch (error) {
    if (error instanceof SpaceItemNotSupportedError) {
      addToast({ title: t('toasts.notSupported'), color: 'warning' })
    } else {
      addToast({
        title: t('toasts.error'),
        description: error instanceof Error ? error.message : undefined,
        color: 'error',
      })
    }
  } finally {
    isDeleting.value = false
  }
}
</script>

<i18n lang="yaml">
de:
  actions:
    delete: Freigabe widerrufen
    confirm: Widerrufen
    cancel: Abbrechen
  confirm:
    title: Freigabe widerrufen?
    body: "{label} — der Zugang wird beim Anbieter widerrufen und für alle Space-Mitglieder entfernt. Diese Aktion kann nicht rückgängig gemacht werden."
  toasts:
    success: Freigabe widerrufen
    notSupported: Löschen dieses Typs wird noch nicht unterstützt
    error: Widerrufen fehlgeschlagen
en:
  actions:
    delete: Revoke share
    confirm: Revoke
    cancel: Cancel
  confirm:
    title: Revoke share?
    body: "{label} — access will be revoked at the provider and removed for all space members. This action cannot be undone."
  toasts:
    success: Share revoked
    notSupported: Deleting this item type is not supported yet
    error: Revoke failed
</i18n>
