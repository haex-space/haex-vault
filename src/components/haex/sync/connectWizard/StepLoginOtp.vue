<template>
  <div class="space-y-4">
    <HaexSyncRecoveryLoginOtp
      :origin-url="originUrl"
      :email="email"
      @recovered="(data) => emit('recovered', data)"
      @change-email="emit('changeEmail')"
    />
  </div>
</template>

<script setup lang="ts">
import type { RecoveryKeyData } from '~/composables/useIdentityRecovery'

defineProps<{
  originUrl: string
  email: string
}>()

const emit = defineEmits<{
  recovered: [
    {
      originUrl: string
      recoveryKeyData: RecoveryKeyData
      session: {
        access_token: string
        refresh_token: string
        expires_in: number
        expires_at: number
      }
      identity: { publicKey: string; did: string; tier: string }
    },
  ]
  changeEmail: []
}>()
</script>
