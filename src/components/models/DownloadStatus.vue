<script setup lang="ts">
/** "x of y downloaded" line of a model row while its download runs (spec 023, research R6). */
import { humanBytes } from '~/lib/models/format'

const props = defineProps<{ modelId: string }>()

const { t } = useI18n()
const downloads = useModelDownloadsStore()

const state = computed(() => downloads.running(props.modelId))
</script>

<template>
  <span v-if="state" class="text-xs text-primary" role="status">
    {{
      t('models.filePicker.downloadProgress', {
        done: humanBytes(state.bytesDownloaded),
        total:
          state.bytesTotal === null
            ? t('models.filePicker.sizeUnknown')
            : humanBytes(state.bytesTotal),
      })
    }}
  </span>
</template>
