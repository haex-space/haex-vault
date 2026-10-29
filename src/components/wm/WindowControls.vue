<script setup lang="ts">
/** Minimize/maximize-restore/close button group (spec 015-workspace-shell, T029/T037, FR-007's
 * right-hand title-bar order). Compact mode hides minimize/maximize — FR-036 offers neither there
 * (a compact window already fills the area and cannot be un-focused onto a visible desktop). */
defineProps<{
  maximized: boolean
  compact: boolean
}>()

const emit = defineEmits<{
  minimize: []
  toggleMaximize: []
  close: []
}>()

const { t } = useI18n()
</script>

<template>
  <div class="flex shrink-0 items-center gap-2" @pointerdown.stop>
    <template v-if="!compact">
      <button
        type="button"
        class="rounded p-1.5 text-muted-foreground hover:bg-warning/20 hover:text-warning"
        :aria-label="t('wm.window.minimize')"
        @click="emit('minimize')"
      >
        <Icon name="lucide:minus" class="h-5 w-5" :aria-hidden="true" />
      </button>
      <button
        type="button"
        class="rounded p-1.5 text-muted-foreground hover:bg-success/20 hover:text-success"
        :aria-label="
          maximized ? t('wm.window.restore') : t('wm.window.maximize')
        "
        @click="emit('toggleMaximize')"
      >
        <Icon
          :name="maximized ? 'lucide:minimize-2' : 'lucide:maximize-2'"
          class="h-5 w-5"
          :aria-hidden="true"
        />
      </button>
    </template>
    <button
      type="button"
      class="rounded p-1.5 text-muted-foreground hover:bg-destructive/20 hover:text-destructive"
      :aria-label="t('wm.window.close')"
      @click="emit('close')"
    >
      <Icon name="lucide:x" class="h-5 w-5" :aria-hidden="true" />
    </button>
  </div>
</template>
