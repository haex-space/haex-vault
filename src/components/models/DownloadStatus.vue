<script setup lang="ts">
/** "x of y downloaded" line of a model row while its download runs (spec 023, research R6). */
import { humanBytes } from '~/lib/models/format'
import { formatSpeed } from '~/lib/transferSpeed'

const props = defineProps<{ modelId: string }>()

const { t } = useI18n()
const downloads = useModelDownloadsStore()

const state = computed(() => downloads.running(props.modelId))
const speed = useTransferSpeed(() => state.value?.bytesDownloaded)

const progress = computed(() => {
  const running = state.value
  if (!running) return ''
  const text = t('models.filePicker.downloadProgress', {
    done: humanBytes(running.bytesDownloaded),
    total:
      running.bytesTotal === null
        ? t('models.filePicker.sizeUnknown')
        : humanBytes(running.bytesTotal),
  })
  return speed.value === null ? text : `${text} · ${formatSpeed(speed.value)}`
})
</script>

<template>
  <span v-if="state" class="text-xs text-primary" role="status">
    {{ progress }}
  </span>
</template>
