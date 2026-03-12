<template>
  <div class="space-y-4">
    <!-- Server URL Selection -->
    <USelectMenu
      v-model="selectedServer"
      :items="serverOptions"
      value-key="value"
      class="w-full"
    />
    <UiInput
      v-if="selectedServer === 'custom'"
      v-model="customServerUrl"
      label="Server URL"
      placeholder="https://..."
      size="lg"
    />

    <!-- Email Input -->
    <UiInput
      v-model="email"
      :label="t('email.label')"
      :description="t('email.description')"
      type="email"
      leading-icon="i-lucide-mail"
      size="lg"
      autofocus
    />
    <UButton
      color="primary"
      size="lg"
      block
      :disabled="!isEmailValid"
      :loading="isLoading"
      @click="onRequestOtpAsync"
    >
      {{ t('email.submit') }}
    </UButton>

    <!-- Error display -->
    <UAlert
      v-if="recoveryError"
      color="error"
      icon="i-lucide-alert-circle"
      :title="t('error.title')"
      :description="recoveryError"
      class="mt-2"
    />
  </div>
</template>

<script setup lang="ts">
import { z } from 'zod'

const { t } = useI18n()
const { serverOptions } = useSyncServerOptions()

const keys = useMagicKeys()
const enter = computed(() => keys.enter?.value ?? false)

whenever(enter, () => {
  if (isEmailValid.value && !isLoading.value) {
    onRequestOtpAsync()
  }
})

const emit = defineEmits<{
  otpRequested: [{ serverUrl: string; email: string }]
}>()

const {
  isLoading,
  error: recoveryError,
  requestOtpAsync,
} = useIdentityRecovery()

// Server selection
const selectedServer = ref('https://sync.haex.space')
const customServerUrl = ref('')
const serverUrl = computed(() =>
  selectedServer.value === 'custom'
    ? customServerUrl.value
    : selectedServer.value,
)

// Email validation with Zod (custom message avoids zodI18n locale lookup)
const emailSchema = z.string().email({ message: 'Invalid email' })

const email = ref('')

const isEmailValid = computed(() => {
  return emailSchema.safeParse(email.value).success
})

const onRequestOtpAsync = async () => {
  const success = await requestOtpAsync(serverUrl.value, email.value)
  if (success) {
    emit('otpRequested', { serverUrl: serverUrl.value, email: email.value })
  }
}
</script>

<i18n lang="yaml">
de:
  email:
    label: E-Mail-Adresse
    description: Die E-Mail-Adresse, mit der du dich beim Sync-Server registriert hast
    submit: Bestätigungscode senden
  error:
    title: Fehler

en:
  email:
    label: Email Address
    description: The email address you used to register with the sync server
    submit: Send verification code
  error:
    title: Error
</i18n>
