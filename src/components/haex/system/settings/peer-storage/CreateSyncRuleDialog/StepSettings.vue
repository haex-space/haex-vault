<template>
  <div class="space-y-4 pt-4">
    <!-- Direction -->
    <div>
      <label class="text-sm font-medium">{{ t('settings.direction') }}</label>
      <div class="flex gap-2 mt-1">
        <UiButton
          :variant="direction === 'one_way' ? 'solid' : 'outline'"
          icon="i-lucide-arrow-right"
          @click="() => { direction = 'one_way' }"
        >
          {{ t('settings.oneWay') }}
        </UiButton>
        <UiButton
          :variant="direction === 'two_way' ? 'solid' : 'outline'"
          icon="i-lucide-arrow-left-right"
          @click="() => { direction = 'two_way' }"
        >
          {{ t('settings.twoWay') }}
        </UiButton>
      </div>
    </div>

    <!-- Delete mode -->
    <UiSelectMenu
      v-model="deleteMode"
      :items="deleteModeOptions"
      :label="t('settings.deleteMode')"
      value-key="value"
    />
  </div>
</template>

<script setup lang="ts">
type DeleteModeOption = { label: string; value: string }

defineProps<{
  deleteModeOptions: DeleteModeOption[]
}>()

const direction = defineModel<'one_way' | 'two_way'>('direction', { required: true })
const deleteMode = defineModel<string>('deleteMode', { required: true })

const { t } = useI18n()
</script>
