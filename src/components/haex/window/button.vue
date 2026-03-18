<template>
  <UTooltip :text="tooltip">
    <button
      class="size-8 shrink-0 rounded-lg flex justify-center transition-colors group"
      :class="variantClasses.buttonClass"
      @click="(e) => $emit('click', e)"
    >
      <UIcon
        :name="icon"
        class="size-4 text-gray-600 dark:text-gray-400"
        :class="variantClasses.iconClass"
      />
    </button>
  </UTooltip>
</template>

<script setup lang="ts">
const props = defineProps<{
  variant: 'close' | 'maximize' | 'minimize' | 'add'
  isMaximized?: boolean
}>()

defineEmits(['click'])

const icon = computed(() => {
  switch (props.variant) {
    case 'close':
      return 'i-heroicons-x-mark'
    case 'maximize':
      return props.isMaximized
        ? 'i-heroicons-arrows-pointing-in'
        : 'i-heroicons-arrows-pointing-out'
    case 'add':
      return 'i-lucide-plus'
    default:
      return 'i-heroicons-minus'
  }
})

const variantClasses = computed(() => {
  switch (props.variant) {
    case 'close':
      return {
        iconClass: 'group-hover:text-error',
        buttonClass: 'hover:bg-error/30 items-center',
      }
    case 'maximize':
      return {
        iconClass: 'group-hover:text-warning',
        buttonClass: 'hover:bg-warning/30 items-center',
      }
    case 'add':
      return {
        iconClass: 'group-hover:text-primary',
        buttonClass: 'hover:bg-primary/30 items-center',
      }
    default:
      return {
        iconClass: 'group-hover:text-success',
        buttonClass: 'hover:bg-success/30 items-end pb-1',
      }
  }
})

const { t } = useI18n()

const tooltip = computed(() => {
  switch (props.variant) {
    case 'close':
      return t('close')
    case 'maximize':
      return props.isMaximized ? t('shrink') : t('maximize')
    case 'add':
      return t('newTab')
    default:
      return t('minimize')
  }
})
</script>

<i18n lang="yaml">
de:
  close: Schließen
  maximize: Maximieren
  shrink: Verkleinern
  minimize: Minimieren
  newTab: Neuer Tab

en:
  close: Close
  maximize: Maximize
  shrink: Shrink
  minimize: Minimize
  newTab: New Tab
</i18n>
