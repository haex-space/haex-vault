<script setup lang="ts">
/**
 * The crumbs of a path in the space the parent gives them, never scrolling (as in COSMIC's file
 * manager): when they do not fit, the folders at the front give way to a "…" whose menu lists
 * them, and the ones at the back stay; the open folder's name is cut short last, and when not even
 * that has room the menu holds all of them. A hidden copy of the whole trail is measured, so the
 * choice follows the width of the box and never the width of what is shown. `ghost` draws the whole
 * trail and nothing else (the window header measures a control's full width that way).
 */
const props = defineProps<{
  segments: readonly { id: string; label: string }[]
  label: string
  testId?: string
  /** A crumb's test id is `<crumbTestId>-<segment id>`. */
  crumbTestId?: string
  disabled?: boolean
  ghost?: boolean
}>()

const emit = defineEmits<{ select: [id: string] }>()

const { t } = useI18n()

/** The smallest width the open folder's name keeps before it moves into the menu too. */
const MIN_LAST = 56

const nav = useTemplateRef<HTMLElement>('nav')
const probe = useTemplateRef<HTMLElement>('probe')
const { width: room } = useElementSize(nav)

/** Index of the first crumb that is drawn; the ones before it are in the menu. */
const from = ref(0)
const first = computed(() => Math.min(from.value, props.segments.length))
const hidden = computed(() => props.segments.slice(0, first.value))
const shown = computed(() => props.segments.slice(first.value))

function fit() {
  if (props.ghost) return
  const available = room.value - 1
  const root = probe.value
  if (!root || available <= 0) return
  const rect = (el: Element) => el.getBoundingClientRect()
  const items = [...root.querySelectorAll('[data-probe-item]')].map(rect)
  const seps = [...root.querySelectorAll('[data-probe-sep]')].map(rect)
  const more = root.querySelector('[data-probe-more]')
  const head = items[0]
  const tail = items[items.length - 1]
  if (!head || !tail || !more) {
    from.value = 0
    return
  }
  const count = items.length
  if (tail.right - head.left <= available) {
    from.value = 0
    return
  }
  const firstSep = seps[0]
  const lastSep = seps[count - 2]
  const gap = firstSep ? firstSep.left - head.right : 0
  const moreWidth = rect(more).width
  // The span from crumb `i` (with the separator before it) to the end of the trail.
  const span = (i: number) =>
    tail.right - ((i === 0 ? head : seps[i - 1])?.left ?? 0)
  const sepWidth = lastSep ? lastSep.width : 0
  let start = count - 1
  while (start > 1 && moreWidth + gap + span(start - 1) <= available) start--
  const onlyLast =
    count === 1 || moreWidth + gap + sepWidth + gap + MIN_LAST <= available
  from.value = start === count - 1 && !onlyLast ? count : start
}

watch([room, () => props.segments], fit, { flush: 'post' })
onMounted(fit)
</script>

<template>
  <div class="relative min-w-0 flex-1">
    <nav
      ref="nav"
      class="overflow-hidden"
      :aria-label="label"
      :data-testid="testId"
    >
      <ShadcnBreadcrumbList class="flex-nowrap">
        <ShadcnBreadcrumbItem v-if="hidden.length" class="shrink-0">
          <ShadcnDropdownMenu>
            <ShadcnDropdownMenuTrigger as-child>
              <button
                type="button"
                class="rounded px-1 hover:text-foreground disabled:opacity-50"
                :disabled="disabled"
                :aria-label="t('files.path.more')"
                :data-testid="crumbTestId ? `${crumbTestId}-more` : undefined"
              >
                …
              </button>
            </ShadcnDropdownMenuTrigger>
            <ShadcnDropdownMenuContent align="start" @close-auto-focus.prevent>
              <ShadcnDropdownMenuItem
                v-for="segment in hidden"
                :key="segment.id"
                class="gap-2"
                :data-testid="
                  crumbTestId ? `${crumbTestId}-${segment.id}` : undefined
                "
                @select="emit('select', segment.id)"
              >
                <Icon name="lucide:folder" class="size-4" :aria-hidden="true" />
                <span class="min-w-0 truncate">{{ segment.label }}</span>
              </ShadcnDropdownMenuItem>
            </ShadcnDropdownMenuContent>
          </ShadcnDropdownMenu>
        </ShadcnBreadcrumbItem>
        <template v-for="(segment, index) in shown" :key="segment.id">
          <ShadcnBreadcrumbSeparator v-if="index > 0 || hidden.length" />
          <ShadcnBreadcrumbItem
            :class="index === shown.length - 1 ? 'min-w-0' : 'shrink-0'"
          >
            <ShadcnBreadcrumbLink
              as="button"
              type="button"
              class="max-w-full truncate rounded px-1 disabled:opacity-50"
              :disabled="disabled"
              :aria-current="index === shown.length - 1 ? 'page' : undefined"
              :data-testid="
                crumbTestId ? `${crumbTestId}-${segment.id}` : undefined
              "
              @click="emit('select', segment.id)"
            >
              {{ segment.label }}
            </ShadcnBreadcrumbLink>
          </ShadcnBreadcrumbItem>
        </template>
      </ShadcnBreadcrumbList>
    </nav>

    <!-- The whole trail and the "…", only to be measured; never seen, never reached. -->
    <div
      v-if="!ghost"
      ref="probe"
      class="pointer-events-none invisible absolute left-0 top-0 w-max"
      aria-hidden="true"
      inert
    >
      <ShadcnBreadcrumbList class="flex-nowrap">
        <ShadcnBreadcrumbItem class="shrink-0" data-probe-more>
          <span class="px-1">…</span>
        </ShadcnBreadcrumbItem>
        <template v-for="(segment, index) in segments" :key="segment.id">
          <ShadcnBreadcrumbSeparator v-if="index > 0" data-probe-sep />
          <ShadcnBreadcrumbItem class="shrink-0" data-probe-item>
            <span class="px-1">{{ segment.label }}</span>
          </ShadcnBreadcrumbItem>
        </template>
      </ShadcnBreadcrumbList>
    </div>
  </div>
</template>
