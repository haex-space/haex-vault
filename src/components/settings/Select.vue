<script setup lang="ts">
/**
 * The select of a settings row (spec 023-settings-app, FR-013): haex-ui's `ShadcnSelect`, so it
 * is readable in both color schemes. Reka's items cannot carry `''`, so an empty value ("Keins",
 * "Wie alle Geräte") travels as a sentinel inside and as `''` outside. An option may carry badges
 * (a state such as "Aktiv"), shown after its label in the list and in the closed select; the
 * closed select shows the text of its option only, so it draws the badged option itself.
 * Attributes such as `id` and `data-testid` go to the trigger, which a row's `<label for>` then
 * names; a `class` replaces the trigger's default width.
 */
defineOptions({ inheritAttrs: false })

/** A short state of an option, shown after its label. */
export type SettingsSelectBadge = {
  text: string
  tone: 'success' | 'warning'
}

export type SettingsSelectOption = {
  value: string
  label: string
  badges?: readonly SettingsSelectBadge[]
  /** An icon name shown before the label; `iconUrl` (an image) wins over it. */
  icon?: string
  iconUrl?: string
  disabled?: boolean
}

const props = defineProps<{
  modelValue: string
  options?: readonly SettingsSelectOption[]
  groups?: readonly {
    key: string
    label: string
    options: readonly SettingsSelectOption[]
  }[]
  disabled?: boolean
}>()

const emit = defineEmits<{
  'update:modelValue': [value: string]
}>()

const attrs = useAttrs()
const triggerAttrs = computed(() => {
  const { class: _class, ...rest } = attrs
  return rest
})
const width = computed(() => (attrs.class as string | undefined) ?? 'w-56')

const NONE = '__none__'
const wire = (value: string) => (value === '' ? NONE : value)

/** The chosen option when it has badges: the closed select draws it, Reka only shows its text. */
const badgedChoice = computed(() =>
  [...(props.options ?? []), ...(props.groups ?? []).flatMap((g) => g.options)]
    .filter((option) => option.value === props.modelValue)
    .find((option) => (option.badges?.length ?? 0) > 0),
)

function onUpdate(value: unknown) {
  const next = String(value)
  emit('update:modelValue', next === NONE ? '' : next)
}
</script>

<template>
  <ShadcnSelect
    :model-value="wire(props.modelValue)"
    :disabled="disabled"
    @update:model-value="onUpdate"
  >
    <ShadcnSelectTrigger
      v-bind="triggerAttrs"
      :class="['h-9 max-w-full bg-background text-sm', width]"
    >
      <ShadcnSelectValue v-if="badgedChoice">
        <SettingsSelectOptionLabel
          :label="badgedChoice.label"
          :badges="badgedChoice.badges"
          :icon="badgedChoice.icon"
          :icon-url="badgedChoice.iconUrl"
        />
      </ShadcnSelectValue>
      <ShadcnSelectValue v-else />
    </ShadcnSelectTrigger>
    <ShadcnSelectContent class="max-h-[60vh]">
      <ShadcnSelectItem
        v-for="option in options ?? []"
        :key="option.value"
        :value="wire(option.value)"
        :disabled="option.disabled"
        :data-value="option.value"
      >
        <SettingsSelectOptionLabel
          :label="option.label"
          :badges="option.badges"
          :icon="option.icon"
          :icon-url="option.iconUrl"
        />
      </ShadcnSelectItem>
      <ShadcnSelectGroup v-for="group in groups ?? []" :key="group.key">
        <ShadcnSelectLabel>{{ group.label }}</ShadcnSelectLabel>
        <ShadcnSelectItem
          v-for="option in group.options"
          :key="option.value"
          :value="wire(option.value)"
          :disabled="option.disabled"
          :data-value="option.value"
        >
          <SettingsSelectOptionLabel
            :label="option.label"
            :badges="option.badges"
            :icon="option.icon"
            :icon-url="option.iconUrl"
          />
        </ShadcnSelectItem>
      </ShadcnSelectGroup>
    </ShadcnSelectContent>
  </ShadcnSelect>
</template>
