<script setup lang="ts">
/**
 * One row of a `SettingsGroup` (spec 023-settings-app, FR-002): optional icon, title and one line
 * of description, the control on the right (default slot). With `to` the whole row is a button
 * that pushes that location and ends in a chevron; with `navigates` it is the same button but
 * emits `select`, for a caller that navigates with more than a path. On a narrow window the control wraps below the
 * text instead of squeezing it. With `fields` the row is one key and its value (an entry's fields):
 * the same everywhere, title above value in a narrow container and side by side from `@lg` on, never
 * depending on how long a value happens to be. `backdrop` renders behind the row, e.g. a download progress bar;
 * `below` spans the row's width under it, e.g. an error.
 */
const props = defineProps<{
  title?: string
  description?: string
  icon?: string
  /** An image shown instead of `icon`. */
  iconUrl?: string
  to?: string
  /** Id of the control the title labels. */
  labelFor?: string
  navigates?: boolean
  /** A key with its value: the layout follows the container's width, not the value's length. */
  fields?: boolean
}>()

const emit = defineEmits<{
  select: []
}>()

const router = useTabRouter()
const isButton = computed(() => Boolean(props.to) || props.navigates)

function onClick() {
  if (!isButton.value) return
  if (props.to) router.push(props.to)
  emit('select')
}

const BASE = computed(() =>
  props.fields
    ? 'relative flex min-h-14 flex-col gap-1 px-4 py-3 @lg:flex-row @lg:items-center @lg:gap-4'
    : 'relative flex min-h-14 flex-wrap items-center gap-x-4 gap-y-2 px-4 py-3',
)
</script>

<template>
  <li class="relative">
    <slot name="backdrop" />
    <component
      :is="isButton ? 'button' : 'div'"
      :type="isButton ? 'button' : undefined"
      :class="[BASE, isButton ? 'w-full text-left hover:bg-foreground/5' : '']"
      @click="onClick"
    >
      <div
        :class="
          fields
            ? 'flex min-w-0 items-center gap-4 @lg:w-2/5 @lg:shrink-0'
            : 'flex min-w-40 flex-1 items-center gap-4'
        "
      >
        <img
          v-if="iconUrl"
          :src="iconUrl"
          alt=""
          class="size-5 shrink-0 object-contain"
          draggable="false"
        />
        <Icon v-else-if="icon" :name="icon" class="size-5 shrink-0" />
        <div class="flex min-w-0 flex-1 flex-col">
          <slot name="title">
            <label v-if="labelFor" :for="labelFor">{{ title }}</label>
            <span v-else>{{ title }}</span>
          </slot>
          <span
            v-if="description || $slots.description"
            class="text-sm text-muted-foreground"
          >
            <slot name="description">{{ description }}</slot>
          </span>
        </div>
      </div>
      <div
        v-if="$slots.default"
        :class="
          fields
            ? 'flex min-w-0 max-w-full items-center gap-2 @lg:flex-1'
            : 'flex w-full max-w-full shrink-0 flex-wrap items-center justify-end gap-2 md:w-auto md:justify-start'
        "
      >
        <slot />
      </div>
      <Icon
        v-if="isButton"
        name="lucide:chevron-right"
        class="size-5 shrink-0 text-muted-foreground"
      />
    </component>
    <div v-if="$slots.below" class="relative px-4 pb-3">
      <slot name="below" />
    </div>
  </li>
</template>
