<template>
  <!-- Active sync progress.
       :key changes when a cycle restart is detected (filesDone
       regresses), forcing remount so the bar doesn't animate
       backwards from the old high to the new low. -->
  <div class="space-y-2">
    <!-- Stats row: active + done counts + speed -->
    <div class="flex items-center justify-between text-xs">
      <span class="text-muted">
        <span
          v-if="progress.activeFiles?.length"
          class="text-primary font-medium"
        >
          {{ progress.activeFiles.length }} {{ t('progress.active') }}
        </span>
        <span v-if="progress.activeFiles?.length && progress.filesDone > 0"> · </span>
        <span v-if="progress.filesDone > 0">
          {{ progress.filesDone }}/{{ progress.filesTotal }} {{ t('progress.done') }}
        </span>
      </span>
      <span v-if="progress.bytesPerSecond > 0" class="text-primary font-medium shrink-0 ml-2 tabular-nums">
        {{ state.formatSpeed(progress.bytesPerSecond) }}
      </span>
    </div>
    <!-- Determinate progress bar (explicit DIVs — UProgress had
         an animated indeterminate look that read like a spinner). -->
    <div class="h-2 w-full rounded-full bg-elevated overflow-hidden">
      <div
        class="h-full bg-primary transition-[width] duration-150 ease-linear"
        :style="{ width: state.percentValue(
          progress.bytesTotal > 0
            ? progress.bytesDone
            : progress.filesDone,
          progress.bytesTotal > 0
            ? progress.bytesTotal
            : progress.filesTotal
        ) + '%' }"
      />
    </div>
    <!-- Bytes transferred + percentage -->
    <div class="flex items-center justify-between text-xs tabular-nums">
      <span v-if="progress.bytesTotal > 0" class="text-muted">
        {{ state.formatBytes(progress.bytesDone) }} / {{ state.formatBytes(progress.bytesTotal) }}
      </span>
      <span v-else class="text-muted">
        {{ progress.filesDone }} / {{ progress.filesTotal }}
      </span>
      <span class="text-primary font-medium">
        {{ state.formatPercent(
          progress.bytesTotal > 0
            ? progress.bytesDone
            : progress.filesDone,
          progress.bytesTotal > 0
            ? progress.bytesTotal
            : progress.filesTotal
        ) }}
      </span>
    </div>
    <!-- Active files list with per-file progress.
         Plain list (no TransitionGroup): the previous fade
         transition with `position: absolute` on leave caused
         leaving rows to overlay entering rows during parallel
         batch turnover, which read as flicker. Per-bar width
         transitions still smooth byte progress animation.
         Files are iterated in stable slot order, so a finishing
         file does not push the remaining ones up — the next
         new file takes the freed slot in place. -->
    <div
      v-if="stableActiveFiles.length"
      class="mt-1 space-y-1.5"
    >
      <div
        v-for="fp in stableActiveFiles"
        :key="fp.path"
        class="space-y-0.5"
      >
        <div class="flex items-center gap-1.5 text-xs text-muted">
          <UIcon
            :name="fp.bytesTotal > 0 && fp.bytesDone >= fp.bytesTotal
              ? 'i-lucide-check'
              : 'i-lucide-arrow-down'"
            class="w-3 h-3 text-primary shrink-0"
          />
          <span class="truncate flex-1">{{ fp.path.split(/[/\\]/).pop() }}</span>
          <span v-if="fp.bytesTotal > 0" class="shrink-0 tabular-nums">
            {{ state.formatBytes(fp.bytesDone) }} / {{ state.formatBytes(fp.bytesTotal) }}
          </span>
          <span v-if="fp.bytesTotal > 0" class="shrink-0 tabular-nums text-primary font-medium">
            {{ fp.bytesDone >= fp.bytesTotal ? t('progress.finalizing') : state.formatPercent(fp.bytesDone, fp.bytesTotal) }}
          </span>
        </div>
        <div
          v-if="fp.bytesTotal > 0"
          class="h-1 w-full rounded-full bg-elevated overflow-hidden"
        >
          <div
            class="h-full bg-primary transition-[width] duration-150 ease-linear"
            :style="{ width: state.percentValue(fp.bytesDone, fp.bytesTotal) + '%' }"
          />
        </div>
      </div>
      <div
        v-if="(progress.activeFiles?.length ?? 0) > stableActiveFiles.length"
        class="text-xs text-muted"
      >
        +{{ (progress.activeFiles?.length ?? 0) - stableActiveFiles.length }} {{ t('progress.moreFiles') }}
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useSyncRulesStateInject } from '@/composables/useSyncRulesState'

interface ActiveFile {
  path: string
  bytesDone: number
  bytesTotal: number
}

interface ProgressShape {
  filesDone: number
  filesTotal: number
  bytesDone: number
  bytesTotal: number
  bytesPerSecond: number
  activeFiles: ActiveFile[]
}

defineProps<{
  progress: ProgressShape
  stableActiveFiles: ActiveFile[]
}>()

const { t } = useI18n()
const state = useSyncRulesStateInject()
</script>
