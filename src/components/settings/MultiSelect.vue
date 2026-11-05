<script setup lang="ts">
/**
 * A select of a settings row that takes several choices (the entries an agent reaches, by tag).
 * "Alle" stands for no choice at all and is what an empty selection shows: choosing it clears the
 * others, choosing another choice drops it, and removing the last choice is "Alle" again. The
 * list stays open while the user ticks. Attributes such as `id` and `data-testid` go to the
 * trigger, which a row's `<label for>` then names; a `class` sets the trigger's width.
 */
defineOptions({ inheritAttrs: false })

const props = defineProps<{
  /** The chosen values; empty means all. */
  modelValue: readonly string[]
  options: readonly string[]
  /** The text of the choice that stands for everything. */
  allLabel: string
  disabled?: boolean
}>()

const emit = defineEmits<{ 'update:modelValue': [value: string[]] }>()

const attrs = useAttrs()
const triggerAttrs = computed(() => {
  const { class: _class, ...rest } = attrs
  return rest
})
const width = computed(() => (attrs.class as string | undefined) ?? 'w-56')

const shown = computed(() =>
  props.modelValue.length === 0 ? props.allLabel : props.modelValue.join(', '),
)

function toggle(value: string, on: boolean) {
  emit(
    'update:modelValue',
    on
      ? [...new Set([...props.modelValue, value])]
      : props.modelValue.filter((chosen) => chosen !== value),
  )
}
</script>

<template>
  <ShadcnDropdownMenu>
    <ShadcnDropdownMenuTrigger as-child :disabled="disabled">
      <button
        v-bind="triggerAttrs"
        type="button"
        :class="[
          'flex h-9 max-w-full items-center justify-between gap-2 rounded-md border border-input bg-background px-3 py-2 text-start text-sm shadow-sm focus:border-primary focus:ring-[3px] focus:ring-primary/50 focus:outline-none disabled:cursor-not-allowed disabled:opacity-50',
          width,
        ]"
      >
        <span class="truncate">{{ shown }}</span>
        <Icon
          name="lucide:chevron-down"
          class="size-4 shrink-0 opacity-50"
          aria-hidden="true"
        />
      </button>
    </ShadcnDropdownMenuTrigger>
    <ShadcnDropdownMenuContent
      align="start"
      class="max-h-[60vh] min-w-(--reka-dropdown-menu-trigger-width)"
    >
      <ShadcnDropdownMenuCheckboxItem
        :model-value="modelValue.length === 0"
        data-value=""
        @update:model-value="emit('update:modelValue', [])"
        @select.prevent
      >
        {{ allLabel }}
      </ShadcnDropdownMenuCheckboxItem>
      <ShadcnDropdownMenuSeparator v-if="options.length > 0" />
      <ShadcnDropdownMenuCheckboxItem
        v-for="option in options"
        :key="option"
        :model-value="modelValue.includes(option)"
        :data-value="option"
        @update:model-value="(on: boolean) => toggle(option, on)"
        @select.prevent
      >
        {{ option }}
      </ShadcnDropdownMenuCheckboxItem>
    </ShadcnDropdownMenuContent>
  </ShadcnDropdownMenu>
</template>
