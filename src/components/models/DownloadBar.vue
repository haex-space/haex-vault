<script setup lang="ts">
/**
 * Progress overlay behind a model row while its download runs (spec 023, research R6). Place it
 * as the first child of a `relative overflow-hidden` row; the row's content needs `relative z-10`.
 */
import { humanBytes } from '~/lib/models/format'

const props = defineProps<{ modelId: string }>()

const { t } = useI18n()
const downloads = useModelDownloadsStore()

const state = computed(() => downloads.downloads[props.modelId])
const percent = computed(() => downloads.downloadPercent(props.modelId))
const label = computed(() =>
  state.value
    ? t('models.filePicker.downloadProgress', {
        done: humanBytes(state.value.bytesDownloaded),
        total:
          state.value.bytesTotal === null
            ? t('models.filePicker.sizeUnknown')
            : humanBytes(state.value.bytesTotal),
      })
    : '',
)
</script>

<template>
  <UiProgress v-if="state" :value="percent" :label="label" variant="backdrop" />
</template>
