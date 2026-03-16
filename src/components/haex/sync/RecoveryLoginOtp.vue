<template>
  <div class="space-y-4">
    <UAlert
      color="info"
      icon="i-lucide-mail"
      :title="t('otp.title')"
      :description="t('otp.description', { email })"
    />
    <div class="flex justify-center">
      <UPinInput
        v-model="otpParts"
        :length="6"
        otp
        type="number"
        size="xl"
        :autofocus="true"
        :ui="{ base: 'h-12 w-12 text-center text-lg' }"
        @complete="onVerifyOtpAsync"
      />
    </div>
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
        @click="emit('changeEmail')"
      >
        {{ t('otp.changeEmail') }}
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
import type { RecoveryKeyData } from '~/composables/useIdentityRecovery'

const { t } = useI18n()
const { add } = useToast()

const props = defineProps<{
  serverUrl: string
  email: string
}>()

const emit = defineEmits<{
  recovered: [{
    serverUrl: string
    recoveryKeyData: RecoveryKeyData
    session: { access_token: string; refresh_token: string; expires_in: number; expires_at: number }
    identity: { publicKey: string; did: string; tier: string }
  }]
  changeEmail: []
}>()

const {
  isLoading,
  error: recoveryError,
  verifyOtpAsync,
  resendOtpAsync,
} = useIdentityRecovery()

const otpParts = ref<number[]>([])

// Reset OTP when email changes (user went back and changed email)
watch(() => props.email, () => {
  otpParts.value = []
})

const onVerifyOtpAsync = async () => {
  const code = otpParts.value.join('')
  const data = await verifyOtpAsync(props.serverUrl, props.email, code)
  if (data) {
    if (data.session && data.identity) {
      emit('recovered', {
        serverUrl: props.serverUrl,
        recoveryKeyData: data,
        session: data.session,
        identity: data.identity,
      })
    }
  } else {
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
  const success = await resendOtpAsync(props.serverUrl, props.email)
  if (success) {
    add({
      title: t('otp.resent'),
      color: 'success',
    })
  }
}
</script>

<i18n lang="yaml">
de:
  otp:
    title: E-Mail-Verifizierung
    description: "Ein 6-stelliger Code wurde an {email} gesendet"
    resend: Code erneut senden
    resent: Code wurde erneut gesendet
    changeEmail: Andere E-Mail verwenden
  error:
    title: Fehler
    noRecoveryKey: Kein Wiederherstellungsschlüssel vorhanden
    noRecoveryKeyDescription: Für dieses Konto wurde kein Wiederherstellungsschlüssel gespeichert. Du benötigst Zugang zu einem Gerät, auf dem deine Identität noch vorhanden ist.

en:
  otp:
    title: Email Verification
    description: "A 6-digit code was sent to {email}"
    resend: Resend code
    resent: Code was resent
    changeEmail: Use different email
  error:
    title: Error
    noRecoveryKey: No recovery key available
    noRecoveryKeyDescription: No recovery key was stored for this account. You need access to a device where your identity still exists.
</i18n>
