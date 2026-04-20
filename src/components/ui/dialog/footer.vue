<template>
  <div class="flex flex-col sm:flex-row gap-4 justify-end w-full">
    <UiButton
      :icon="cancelIcon ?? 'i-heroicons-x-mark'"
      :label="cancelLabel"
      :color="cancelColor ?? 'neutral'"
      variant="outline"
      :disabled="loading"
      class="w-full sm:w-auto"
      @click="emit('cancel')"
    />
    <UiButton
      :data-testid="confirmTestId"
      :icon="confirmIcon ?? 'i-heroicons-check'"
      :label="confirmLabel"
      :color="confirmColor ?? 'primary'"
      :loading="loading"
      :disabled="confirmDisabled"
      class="w-full sm:w-auto"
      @click="emit('confirm')"
    />
  </div>
</template>

<script setup lang="ts">
import type { ButtonProps } from '@nuxt/ui'

withDefaults(
  defineProps<{
    cancelLabel: string
    confirmLabel: string
    cancelIcon?: string
    confirmIcon?: string
    cancelColor?: ButtonProps['color']
    confirmColor?: ButtonProps['color']
    /** Shows spinner on confirm, disables both buttons */
    loading?: boolean
    confirmDisabled?: boolean
    confirmTestId?: string
  }>(),
  {
    cancelIcon: undefined,
    confirmIcon: undefined,
    cancelColor: undefined,
    confirmColor: undefined,
    loading: false,
    confirmDisabled: false,
    confirmTestId: undefined,
  },
)

const emit = defineEmits<{
  cancel: []
  confirm: []
}>()
</script>
