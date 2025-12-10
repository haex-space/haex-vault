<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
    :description="t('description')"
  >
    <div class="space-y-4 pt-2">
      <UAlert
        color="info"
        icon="i-lucide-info"
        :description="t('info')"
      />

      <UiInputPassword
        v-model="serverPassword"
        v-model:errors="serverPasswordErrors"
        :label="t('serverPassword.label')"
        autocomplete="off"
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
          :disabled="loading || !serverPassword"
          @click="onConfirm"
        >
          {{ t('confirm') }}
        </UButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import type { SelectHaexSyncBackends } from '~/database/schemas'

const props = defineProps<{
  backend: SelectHaexSyncBackends | null
  loading: boolean
}>()

const emit = defineEmits<{
  confirm: [serverPassword: string]
}>()

const open = defineModel<boolean>('open', { default: false })

const { t } = useI18n()

const serverPassword = ref('')
const serverPasswordErrors = ref<string[]>([])

// Reset form when dialog opens
watch(open, (isOpen) => {
  if (isOpen) {
    serverPassword.value = ''
    serverPasswordErrors.value = []
  }
})

const onConfirm = () => {
  if (!serverPassword.value) {
    serverPasswordErrors.value = [t('serverPassword.required')]
    return
  }

  emit('confirm', serverPassword.value)
}
</script>

<i18n lang="yaml">
de:
  title: Daten erneut hochladen
  description: Lade alle lokalen Daten auf den Server hoch
  info: Gib dein Server-Passwort ein, um den Vault-Schlüssel erneut zu verschlüsseln und alle lokalen Daten auf den Server hochzuladen.
  serverPassword:
    label: Server-Passwort
    required: Server-Passwort ist erforderlich
  cancel: Abbrechen
  confirm: Hochladen
en:
  title: Re-upload Data
  description: Upload all local data to the server
  info: Enter your server password to re-encrypt the vault key and upload all local data to the server.
  serverPassword:
    label: Server Password
    required: Server password is required
  cancel: Cancel
  confirm: Upload
</i18n>
