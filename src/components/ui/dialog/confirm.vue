<template>
  <UModal
    v-model:open="open"
    :title="title"
    :description="description"
  >
    <template
      v-for="(_, name) in $slots"
      :key="name"
      #[name]="slotData"
    >
      <slot
        :name="name"
        v-bind="slotData"
      />
    </template>

    <template #footer>
      <div class="flex flex-col sm:flex-row gap-4 justify-end w-full">
        <UiButton
          :icon="abortIcon"
          :label="abortLabel || t('abort')"
          block

          color="error"
          variant="outline"
          @click="open = false"
        />
        <UiButton
          :icon="confirmIcon"
          :label="confirmLabel || t('confirm')"
          :disabled="confirmDisabled"
          block

          color="primary"
          variant="solid"
          @click="$emit('confirm')"
        />
      </div>
    </template>
  </UModal>
</template>

<script setup lang="ts">
withDefaults(
  defineProps<{
    abortIcon?: string
    abortLabel?: string
    confirmIcon?: string
    confirmLabel?: string
    confirmDisabled?: boolean
    description?: string
    title?: string
  }>(),
  {
    abortIcon: 'mdi:close',
    abortLabel: '',
    confirmIcon: 'mdi:check',
    confirmLabel: '',
    confirmDisabled: false,
    description: ' ',
    title: ' ',
  },
)

const open = defineModel<boolean>('open', { default: false })

const { t } = useI18n()
defineEmits(['confirm'])
</script>

<i18n lang="yaml">
de:
  abort: Abbrechen
  confirm: Bestätigen

en:
  abort: Abort
  confirm: Confirm
</i18n>
