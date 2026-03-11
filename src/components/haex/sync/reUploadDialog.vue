<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
    :description="t('description')"
  >
    <div class="pt-2">
      <UAlert
        color="info"
        icon="i-lucide-info"
        :description="t('info')"
      />
    </div>

    <template #footer>
      <div class="flex justify-between w-full">
        <UButton
          color="neutral"
          variant="outline"
          :disabled="loading"
          @click="open = false"
        >
          {{ t('cancel') }}
        </UButton>

        <UButton
          color="primary"
          icon="i-lucide-upload"
          :loading="loading"
          :disabled="loading"
          @click="emit('confirm')"
        >
          {{ t('confirm') }}
        </UButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
defineProps<{
  loading: boolean
}>()

const emit = defineEmits<{
  confirm: []
}>()

const open = defineModel<boolean>('open', { default: false })

const { t } = useI18n()
</script>

<i18n lang="yaml">
de:
  title: Daten erneut hochladen
  description: Lade alle lokalen Daten auf den Server hoch
  info: Der Vault-Schlüssel wird erneut verschlüsselt und zusammen mit allen lokalen Daten auf den Server hochgeladen.
  cancel: Abbrechen
  confirm: Hochladen
en:
  title: Re-upload Data
  description: Upload all local data to the server
  info: The vault key will be re-encrypted and uploaded to the server along with all local data.
  cancel: Cancel
  confirm: Upload
</i18n>
