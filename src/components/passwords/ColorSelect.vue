<script setup lang="ts">
/**
 * A select for the colour of a tag: "Standard" (no colour of its own) first, then the picker
 * colours of `ENTRY_COLORS` by name, each with a dot. Reka's items cannot carry `''`, so "no
 * colour" travels as a sentinel inside and as `null` outside. Attributes such as `data-testid`
 * go to the trigger.
 */
import { ENTRY_COLORS } from '~/lib/passwords/icons'

defineOptions({ inheritAttrs: false })

const props = defineProps<{ modelValue: string | null }>()
const emit = defineEmits<{ 'update:modelValue': [value: string | null] }>()

const { t } = useI18n()

/** The names of `ENTRY_COLORS`, in the same order. */
const NAMES = [
  'red',
  'orange',
  'yellow',
  'green',
  'teal',
  'blue',
  'purple',
  'pink',
  'gray',
] as const

const NONE = '__none__'

const options = computed(() => [
  { value: NONE, color: null, label: t('passwords.colors.default') },
  ...ENTRY_COLORS.map((color, index) => ({
    value: color as string,
    color: color as string,
    label: t(`passwords.colors.${NAMES[index]}`),
  })),
])

/** A colour that is not in the list (set elsewhere) is shown as it is, so nothing looks unset. */
const chosen = computed(
  () =>
    options.value.find((o) => o.value === (props.modelValue ?? NONE)) ?? {
      value: props.modelValue ?? NONE,
      color: props.modelValue,
      label: props.modelValue ?? '',
    },
)

function onUpdate(value: unknown) {
  const next = String(value)
  emit('update:modelValue', next === NONE ? null : next)
}
</script>

<template>
  <ShadcnSelect :model-value="chosen.value" @update:model-value="onUpdate">
    <ShadcnSelectTrigger
      v-bind="$attrs"
      class="h-9 w-40 max-w-full bg-background text-sm"
      :aria-label="t('passwords.fields.color')"
    >
      <ShadcnSelectValue>
        <span class="flex min-w-0 items-center gap-2">
          <span
            class="size-3.5 shrink-0 rounded-full"
            :class="
              chosen.color ? '' : 'border border-dashed border-muted-foreground'
            "
            :style="
              chosen.color ? { backgroundColor: chosen.color } : undefined
            "
          />
          <span class="truncate">{{ chosen.label }}</span>
        </span>
      </ShadcnSelectValue>
    </ShadcnSelectTrigger>
    <ShadcnSelectContent class="max-h-[60vh]">
      <ShadcnSelectItem
        v-for="option in options"
        :key="option.value"
        :value="option.value"
        :data-value="option.color ?? ''"
      >
        <span class="flex min-w-0 items-center gap-2">
          <span
            class="size-3.5 shrink-0 rounded-full"
            :class="
              option.color ? '' : 'border border-dashed border-muted-foreground'
            "
            :style="
              option.color ? { backgroundColor: option.color } : undefined
            "
          />
          <span class="truncate">{{ option.label }}</span>
        </span>
      </ShadcnSelectItem>
    </ShadcnSelectContent>
  </ShadcnSelect>
</template>
