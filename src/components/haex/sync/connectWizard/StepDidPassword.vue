<template>
  <div class="space-y-4">
    <p class="text-sm text-muted">
      {{ t('steps.didPassword.description') }}
    </p>

    <UiInputPassword
      v-model="passwordModel"
      :label="t('steps.didPassword.label')"
      leading-icon="i-lucide-fingerprint"
      class="w-full"
    />

    <p
      v-if="error"
      class="text-sm text-error"
    >
      {{ error }}
    </p>
  </div>
</template>

<script setup lang="ts">
const { t } = useI18n()

defineProps<{
  error: string
}>()

const passwordModel = defineModel<string>('password', { required: true })
</script>
