<script setup lang="ts">
/**
 * A choice inside a `SettingsGroup` (spec 023-settings-app, FR-002, FR-021): the whole row is the
 * label of its radio button or checkbox (haex-ui's `ShadcnRadioGroupItem` / `ShadcnCheckbox`), so
 * a click anywhere on it chooses. A radio row needs a `SettingsGroup radio`, which owns the chosen
 * `value`; a checkbox row reports `change`, saving is the caller's. Attributes such as
 * `data-testid` go to the control. The slot `trailing` sits at the end of the row (a preview).
 */
defineOptions({ inheritAttrs: false })

defineProps<{
  type: 'radio' | 'checkbox'
  value?: string | number
  checked?: boolean
  disabled?: boolean
  title: string
  description?: string
}>()

const emit = defineEmits<{
  change: [checked: boolean]
}>()
</script>

<template>
  <li>
    <label
      class="flex min-h-14 cursor-pointer items-center gap-4 px-4 py-3 hover:bg-foreground/5 has-disabled:cursor-default has-disabled:opacity-60"
    >
      <ShadcnCheckbox
        v-if="type === 'checkbox'"
        :model-value="checked"
        :disabled="disabled"
        v-bind="$attrs"
        @update:model-value="emit('change', $event === true)"
      />
      <ShadcnRadioGroupItem
        v-else
        :value="value ?? ''"
        :disabled="disabled"
        v-bind="$attrs"
      />
      <span class="flex min-w-0 flex-1 flex-col">
        <span>{{ title }}</span>
        <span v-if="description" class="text-sm text-muted-foreground">
          {{ description }}
        </span>
      </span>
      <slot name="trailing" />
    </label>
  </li>
</template>
