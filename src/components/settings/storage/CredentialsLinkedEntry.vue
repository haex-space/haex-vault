<script setup lang="ts">
/**
 * The password manager entry a saved connection takes its credentials from (spec 052 US2,
 * FR-007): its title, never a value, and a way to open it in the password manager. The path
 * carries only the entry's opaque id (spec 034 FR-039).
 */
import type { CredentialsEntry } from '@bindings/CredentialsEntry'

defineProps<{ entry: CredentialsEntry }>()
const { t } = useI18n()
const { openApp } = useWmTab()
</script>

<template>
  <SettingsRow
    :title="t('settings.storage.credentialsLinked')"
    :description="entry.title || t('passwords.untitled')"
    data-testid="storage-credentials-entry"
  >
    <UiButton
      type="button"
      size="sm"
      variant="outline"
      data-testid="storage-open-credentials-entry"
      @click="openApp('system.passwords', `/entry/${entry.id}`)"
    >
      {{ t('settings.storage.openCredentialsEntry') }}
    </UiButton>
  </SettingsRow>
</template>
