<template>
  <div class="space-y-4">
    <HaexSyncRecoveryLogin @otp-requested="(data) => emit('otpRequested', data)" />
  </div>
</template>

<script setup lang="ts">
const emit = defineEmits<{
  otpRequested: [{ originUrl: string; email: string }]
}>()
</script>
