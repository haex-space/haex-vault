<template>
  <div
    v-if="isVisible"
    class="fixed inset-0 z-50 flex items-center justify-center bg-base-100/95 backdrop-blur-sm"
  >
    <div class="flex flex-col items-center gap-6 max-w-md text-center px-4">
      <div class="relative">
        <div class="loading loading-spinner loading-lg text-primary"/>
      </div>
      <div class="space-y-2">
        <h2 class="text-2xl font-bold">{{ t('title') }}</h2>
        <p class="text-base-content/70">{{ t('description') }}</p>
      </div>
      <div
        v-if="progress"
        class="w-full space-y-2"
      >
        <div class="flex justify-between text-sm">
          <span>{{ t('progress') }}</span>
          <span>{{ progress.synced }} / {{ progress.total }}</span>
        </div>
        <progress
          class="progress progress-primary w-full"
          :value="progress.synced"
          :max="progress.total"
        />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
const { t } = useI18n()

interface SyncProgress {
  synced: number
  total: number
}

defineProps<{
  isVisible: boolean
  progress?: SyncProgress
}>()
</script>

<i18n lang="yaml">
de:
  title: Synchronisiere mit Server
  description: Bitte warte, während die Daten vom Server heruntergeladen werden. Dies kann einen Moment dauern.
  progress: Fortschritt

en:
  title: Syncing with Server
  description: Please wait while data is being downloaded from the server. This may take a moment.
  progress: Progress
</i18n>
