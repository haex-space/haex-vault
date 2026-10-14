<script setup lang="ts">
/**
 * The credentials of a saved connection are gone or no longer fit (spec 052 US3, FR-010): the
 * entry was deleted, moved to the trash, or lacks a value the protocol needs. The user chooses
 * another entry of the password manager or types new credentials; both set the form's source.
 */
const emit = defineEmits<{ choose: [source: 'entry' | 'new'] }>()
const { t } = useI18n()
</script>

<template>
  <li
    class="flex flex-col gap-2 px-4 py-3"
    role="alert"
    data-testid="storage-credentials-missing-note"
  >
    <p class="text-sm text-destructive">
      {{ t('settings.storage.credentialsUnavailable.missing') }}
    </p>
    <div class="flex flex-wrap gap-2">
      <UiButton
        type="button"
        size="sm"
        variant="outline"
        data-testid="storage-credentials-choose-other"
        @click="emit('choose', 'entry')"
      >
        {{ t('settings.storage.credentialsMissing.chooseOther') }}
      </UiButton>
      <UiButton
        type="button"
        size="sm"
        variant="outline"
        data-testid="storage-credentials-enter-new"
        @click="emit('choose', 'new')"
      >
        {{ t('settings.storage.credentialsMissing.enterNew') }}
      </UiButton>
    </div>
  </li>
</template>
