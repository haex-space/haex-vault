<script setup lang="ts">
/**
 * The frame of a full-tab page of an app (a password's view or editor, the trash, the generator,
 * the import, a pass in Wallet): a scrolling area, a centred column, and a header row with the
 * back button, the title and the page's own buttons. One component so that every page puts the
 * back button at the same spot and a change of the frame is made once.
 *
 * - `title` is a plain heading; a page whose heading is more than text (an icon, a badge)
 *   fills the `title` slot with its own `h1` instead.
 * - `actions` holds the buttons on the right of the header.
 * - The default slot is the body below the header. Attributes of the component (`class`,
 *   `data-testid`, `@submit`) land on the column, which is a `form` with `as="form"`.
 * - `overlays` is for dialogs and the like: they sit beside the column, inside the scrolling
 *   area, and take no part in its spacing.
 */
const props = withDefaults(
  defineProps<{
    /** Label of the back button (screen readers, tooltip). */
    backLabel: string
    backTestid?: string
    backDisabled?: boolean
    title?: string
    titleTestid?: string
    /** The element of the column: a `form` when the body is one. */
    as?: 'div' | 'form'
    /** Space between the header and the blocks of the body. */
    gap?: 'sm' | 'md' | 'lg'
  }>(),
  { as: 'div', gap: 'md' },
)

const emit = defineEmits<{ back: [] }>()

defineOptions({ inheritAttrs: false })

const GAP_CLASS = { sm: 'gap-3', md: 'gap-4', lg: 'gap-5' } as const
</script>

<template>
  <div class="min-h-0 flex-1 overflow-y-auto px-4 pb-6 @md:px-6">
    <component
      :is="props.as"
      v-bind="$attrs"
      class="mx-auto flex w-full max-w-3xl flex-col"
      :class="GAP_CLASS[props.gap]"
    >
      <div class="flex items-center gap-2 pt-1">
        <UiButtonBack
          :label="props.backLabel"
          class="-ml-2"
          :data-testid="props.backTestid"
          :disabled="props.backDisabled"
          @click="emit('back')"
        />
        <div v-if="$slots.title" class="min-w-0 flex-1">
          <slot name="title" />
        </div>
        <h1
          v-else
          class="min-w-0 flex-1 truncate text-2xl font-bold"
          :data-testid="props.titleTestid"
        >
          {{ props.title }}
        </h1>
        <slot name="actions" />
      </div>

      <slot />
    </component>

    <slot name="overlays" />
  </div>
</template>
