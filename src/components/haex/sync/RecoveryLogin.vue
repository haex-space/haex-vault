<template>
  <div class="space-y-4">
    <!-- Server URL Selection (reuse existing pattern) -->
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

    <!-- Phase 1: Email Input -->
    <div v-if="phase === 'email'" class="space-y-4">
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
    </div>

    <!-- Phase 2: OTP Verification -->
    <div v-else-if="phase === 'otp'" class="space-y-4">
      <UAlert
        color="info"
        icon="i-lucide-mail"
        :title="t('otp.title')"
        :description="t('otp.description', { email })"
      />
      <UPinInput
        v-model="otpParts"
        :length="6"
        otp
        type="number"
        size="xl"
        :autofocus="true"
        class="justify-center"
        :ui="{ base: 'w-12 h-12 text-center text-lg' }"
        @complete="onVerifyOtpAsync"
      />
      <div class="flex justify-between items-center">
        <UButton
          variant="link"
          size="xs"
          @click="onResendAsync"
        >
          {{ t('otp.resend') }}
        </UButton>
        <UButton
          variant="link"
          size="xs"
          @click="phase = 'email'"
        >
          {{ t('otp.changeEmail') }}
        </UButton>
      </div>
    </div>

    <!-- Phase 3: Vault Password to Decrypt Key -->
    <div v-else-if="phase === 'password'" class="space-y-4">
      <UAlert
        color="success"
        icon="i-lucide-check-circle"
        :title="t('password.verified')"
      />
      <UiInputPassword
        v-model="vaultPassword"
        :label="t('password.label')"
        :description="t('password.description')"
        leading-icon="i-lucide-lock"
        size="lg"
        autofocus
      />
      <UButton
        color="primary"
        size="lg"
        block
        :disabled="!vaultPassword"
        :loading="isLoading"
        @click="onDecryptAsync"
      >
        {{ t('password.submit') }}
      </UButton>
    </div>

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
import type { RecoveryKeyData } from '~/composables/useIdentityRecovery'

const { t } = useI18n()
const { serverOptions } = useSyncServerOptions()
const { add } = useToast()

const emit = defineEmits<{
  recovered: [{ identityId: string; serverUrl: string; vaultPassword: string }]
}>()

const {
  isLoading,
  error: recoveryError,
  requestOtpAsync,
  verifyOtpAsync,
  decryptAndImportAsync,
  resendOtpAsync,
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

// State machine
const phase = ref<'email' | 'otp' | 'password'>('email')
const email = ref('')
const otpParts = ref<number[]>([])
const vaultPassword = ref('')
const recoveredKeyData = ref<RecoveryKeyData | null>(null)

const isEmailValid = computed(() => {
  return emailSchema.safeParse(email.value).success
})

const onRequestOtpAsync = async () => {
  const success = await requestOtpAsync(serverUrl.value, email.value)
  if (success) {
    phase.value = 'otp'
    otpParts.value = []
  }
}

const onVerifyOtpAsync = async () => {
  const code = otpParts.value.join('')
  const data = await verifyOtpAsync(serverUrl.value, email.value, code)
  if (data) {
    recoveredKeyData.value = data
    phase.value = 'password'
  } else {
    // Check if the error is specifically about missing recovery key
    if (recoveryError.value?.includes('No recovery key')) {
      add({
        title: t('error.noRecoveryKey'),
        description: t('error.noRecoveryKeyDescription'),
        color: 'warning',
      })
    }
    otpParts.value = []
  }
}

const onResendAsync = async () => {
  const success = await resendOtpAsync(serverUrl.value, email.value)
  if (success) {
    add({
      title: t('otp.resent'),
      color: 'success',
    })
  }
}

const onDecryptAsync = async () => {
  if (!recoveredKeyData.value) return

  const identityId = await decryptAndImportAsync(
    recoveredKeyData.value,
    vaultPassword.value,
  )

  if (identityId) {
    emit('recovered', {
      identityId,
      serverUrl: serverUrl.value,
      vaultPassword: vaultPassword.value,
    })
  }
}
</script>

<i18n lang="yaml">
de:
  email:
    label: E-Mail-Adresse
    description: Die E-Mail-Adresse, mit der du dich beim Sync-Server registriert hast
    submit: Bestätigungscode senden
  otp:
    title: E-Mail-Verifizierung
    description: "Ein 6-stelliger Code wurde an {email} gesendet"
    resend: Code erneut senden
    resent: Code wurde erneut gesendet
    changeEmail: Andere E-Mail verwenden
  password:
    verified: E-Mail erfolgreich verifiziert
    label: Vault-Passwort
    description: Gib dein Vault-Passwort ein, um deinen Schlüssel zu entschlüsseln
    submit: Schlüssel entschlüsseln
  error:
    title: Fehler
    noRecoveryKey: Kein Wiederherstellungsschlüssel vorhanden
    noRecoveryKeyDescription: Für dieses Konto wurde kein Wiederherstellungsschlüssel gespeichert. Du benötigst Zugang zu einem Gerät, auf dem deine Identität noch vorhanden ist.

en:
  email:
    label: Email Address
    description: The email address you used to register with the sync server
    submit: Send verification code
  otp:
    title: Email Verification
    description: "A 6-digit code was sent to {email}"
    resend: Resend code
    resent: Code was resent
    changeEmail: Use different email
  password:
    verified: Email verified successfully
    label: Vault Password
    description: Enter your vault password to decrypt your key
    submit: Decrypt key
  error:
    title: Error
    noRecoveryKey: No recovery key available
    noRecoveryKeyDescription: No recovery key was stored for this account. You need access to a device where your identity still exists.
</i18n>
