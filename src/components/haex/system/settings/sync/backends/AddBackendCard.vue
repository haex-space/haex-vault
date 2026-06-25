<template>
  <UCard class="relative">
    <!-- Loading Overlay -->
    <div
      v-if="loading"
      class="absolute inset-0 z-10 flex items-center justify-center bg-default/80 backdrop-blur-sm rounded-lg"
    >
      <div class="flex flex-col items-center gap-3">
        <div class="loading loading-spinner loading-lg text-primary" />
        <span class="text-sm text-muted">
          {{ t('addBackend.connecting') }}
        </span>
      </div>
    </div>

    <template #header>
      <div class="flex justify-between px-1">
        <h3 class="text-lg font-semibold">
          {{ t('addBackend.title') }}
        </h3>

        <UiButton
          icon="mdi-close"
          variant="ghost"
          color="neutral"
          :disabled="loading"
          @click="$emit('cancel')"
        />
      </div>
    </template>

    <!-- Verification Code Input -->
    <div
      v-if="verificationPending"
      class="space-y-4"
    >
      <UAlert
        color="info"
        icon="i-lucide-mail"
        :title="t('verification.title')"
        :description="t('verification.description')"
      />

      <div class="flex justify-center">
        <UPinInput
          v-model="verificationCodeParts"
          :length="6"
          otp
          type="number"
          size="xl"
          :autofocus="true"
          :ui="{ base: 'w-12 h-12 text-center text-lg' }"
          @complete="$emit('verify')"
        />
      </div>

      <UButton
        variant="link"
        :disabled="loading"
        @click="$emit('resend')"
      >
        {{ t('verification.resend') }}
      </UButton>
    </div>

    <!-- Add Backend Form -->
    <HaexSyncAddBackend
      v-else
      v-model:identity-id="identityId"
      v-model:approved-claims="approvedClaims"
      v-model:origin-url="originUrl"
      :items="serverOptions"
      @keydown.enter.prevent="$emit('submit')"
    />

    <template #footer>
      <div class="flex justify-between">
        <UButton
          color="neutral"
          variant="outline"
          :disabled="loading"
          @click="$emit('cancel')"
        >
          {{ t('actions.back') }}
        </UButton>

        <UiButton
          v-if="verificationPending"
          icon="mdi-check"
          :disabled="loading || verificationCode.length !== 6"
          @click="$emit('verify')"
        >
          <span class="hidden @sm:inline">
            {{ t('verification.verify') }}
          </span>
        </UiButton>
        <UiButton
          v-else
          icon="mdi-plus"
          :disabled="loading"
          data-testid="sync-submit-button"
          @click="$emit('submit')"
        >
          <span class="hidden @sm:inline">
            {{ t('actions.add') }}
          </span>
        </UiButton>
      </div>
    </template>
  </UCard>
</template>

<script setup lang="ts">
interface VerificationPending {
  did: string
  originUrl: string
  identityId: string
  approvedClaims: Record<string, string>
}

defineProps<{
  loading: boolean
  verificationPending: VerificationPending | null
  serverOptions: any[]
}>()

defineEmits<{
  submit: []
  cancel: []
  verify: []
  resend: []
}>()

const identityId = defineModel<string>('identityId', { required: true })
const originUrl = defineModel<string>('originUrl', { required: true })
const approvedClaims = defineModel<Record<string, string>>('approvedClaims', {
  required: true,
})
const verificationCodeParts = defineModel<number[]>('verificationCodeParts', {
  required: true,
})

const { t } = useI18n()

const verificationCode = computed(() => verificationCodeParts.value.join(''))
</script>
