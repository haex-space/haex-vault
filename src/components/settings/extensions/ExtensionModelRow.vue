<script setup lang="ts">
/**
 * One model of an extension's model list (spec 055 US2): its name and details, a badge when it is
 * the extension's default and the select of its effort. `ExtensionModelsView` holds the state and
 * runs the changes.
 */
import type { ExtensionModelEntry } from '@bindings/ExtensionModelEntry'
import type { SettingsSelectOption } from '~/components/settings/Select.vue'
import {
  ceilingChoices,
  effortLabelKey,
  placeLabelKey,
} from '~/lib/extensions/modelGrants'

const props = defineProps<{
  model: ExtensionModelEntry
  /** The provider is named in the details; inside its own group it needs no repeating. */
  showProvider?: boolean
}>()

const emit = defineEmits<{
  ceiling: [effort: string]
}>()

const { t } = useI18n()

/**
 * The efforts of the model: "Nicht erlaubt" (`''`) and the efforts it offers. A model without a
 * choice of effort has only `none`, which says "Erlaubt".
 */
const ceilingOptions = computed<SettingsSelectOption[]>(() => [
  { value: '', label: t('settings.extensions.models.notAllowed') },
  ...ceilingChoices(props.model.efforts, props.model.ceiling).map((value) => ({
    value,
    label:
      props.model.efforts.length === 0
        ? t('settings.extensions.models.allowed')
        : t(effortLabelKey(value)),
  })),
])

const detail = computed(() => {
  const { model } = props
  const parts = props.showProvider ? [model.provider] : []
  parts.push(t(placeLabelKey(model.place)))
  if (model.takesImages) parts.push(t('settings.extensions.models.takesImages'))
  if (model.agent) parts.push(t('settings.extensions.models.agent'))
  if (model.denied) parts.push(t('settings.extensions.models.denied'))
  if (model.held) parts.push(t('settings.extensions.models.held'))
  return parts.join(' · ')
})
</script>

<template>
  <li
    class="flex flex-wrap items-center gap-x-4 gap-y-2 px-4 py-3"
    :data-model="model.name"
  >
    <div class="flex min-w-48 flex-1 flex-col">
      <span class="flex items-center gap-2 text-sm">
        {{ model.name }}
        <span
          v-if="model.isDefault"
          class="rounded-full bg-primary/10 px-2 py-0.5 text-xs text-primary"
          data-testid="extension-model-default-badge"
        >
          {{ t('settings.extensions.models.defaultBadge') }}
        </span>
      </span>
      <span class="text-xs text-muted-foreground">{{ detail }}</span>
    </div>
    <SettingsSelect
      :model-value="model.ceiling ?? ''"
      :options="ceilingOptions"
      :aria-label="
        t('settings.extensions.models.ceiling', { name: model.name })
      "
      class="w-44"
      :data-testid="`extension-model-ceiling-${model.name}`"
      @update:model-value="emit('ceiling', $event)"
    />
  </li>
</template>
