<script setup lang="ts">
/**
 * The loader over a folder that takes a moment to list, mostly a storage or another device answering
 * over the network: holzi's squirrel gnaws at an acorn over the dimmed old entries, so a click shows
 * it landed while the answer is still on its way. It appears only after a short delay, so a local
 * folder that lists at once does not flash it, and it lets clicks through to the list below.
 *
 * The animation is a sprite of four frames side by side (`assets/loader/squirrel-nut.webp`, made
 * from the app icon with an image model), stepped through like a flip book.
 */
import sprite from '~/assets/loader/squirrel-nut.webp'

const props = defineProps<{ loading: boolean }>()

const { t } = useI18n()

const DELAY_MS = 200
const shown = ref(false)
let timer: ReturnType<typeof setTimeout> | null = null

watch(
  () => props.loading,
  (loading) => {
    if (timer) clearTimeout(timer)
    timer = null
    if (!loading) {
      shown.value = false
      return
    }
    timer = setTimeout(() => {
      timer = null
      shown.value = true
    }, DELAY_MS)
  },
  { immediate: true },
)
onBeforeUnmount(() => {
  if (timer) clearTimeout(timer)
})
</script>

<template>
  <Transition
    enter-active-class="transition-opacity duration-150"
    enter-from-class="opacity-0"
  >
    <div
      v-if="shown"
      class="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-3 bg-background/60"
      role="status"
      data-testid="files-loading"
    >
      <div class="size-20 overflow-hidden">
        <img
          :src="sprite"
          alt=""
          class="gnaw h-full w-[400%] max-w-none"
          draggable="false"
        />
      </div>
      <span class="text-sm text-muted-foreground">{{
        t('files.loading')
      }}</span>
    </div>
  </Transition>
</template>

<style scoped>
/* Four frames, about six a second: each step moves the strip by one frame. */
.gnaw {
  animation: gnaw 660ms steps(4) infinite;
}

@keyframes gnaw {
  to {
    transform: translateX(-100%);
  }
}

/* Without motion the squirrel stays on its first frame and only fades in and out. */
@keyframes breathe {
  50% {
    opacity: 0.5;
  }
}

@media (prefers-reduced-motion: reduce) {
  .gnaw {
    animation: breathe 1.6s ease-in-out infinite;
  }
}
</style>
