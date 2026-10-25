<script setup lang="ts">
/**
 * The choice in the question of a job (spec 055 US2, T036): which of the models that fit the job
 * the extension may use, and up to which effort. holzi offers only models that can do what the job
 * needs, the one it proposes first; the user's pick goes back with the answer.
 */
import {
  ceilingChoices,
  effortLabelKey,
  placeLabelKey,
} from '~/lib/extensions/modelGrants'

const props = defineProps<{
  models: {
    id: string
    name: string
    provider: string
    place: string
    agent?: boolean
    efforts: string[]
  }[]
}>()
const modelId = defineModel<string>('modelId', { required: true })
const effort = defineModel<string>('effort', { required: true })
const { t } = useI18n()

/** The efforts of the chosen model; a model without a choice of effort shows no select. */
const offered = computed(
  () => props.models.find((m) => m.id === modelId.value)?.efforts ?? [],
)
const effortOptions = computed(() =>
  ceilingChoices(offered.value).map((value) => ({
    value,
    label: t(effortLabelKey(value)),
  })),
)
</script>

<template>
  <div class="space-y-3">
    <p class="text-sm">{{ t('extensions.permissionRequest.model.intro') }}</p>
    <ShadcnRadioGroup
      v-model="modelId"
      class="gap-2"
      :aria-label="t('extensions.permissionRequest.model.models')"
    >
      <label
        v-for="model in models"
        :key="model.id"
        class="flex cursor-pointer items-start gap-2 text-sm"
      >
        <ShadcnRadioGroupItem
          :value="model.id"
          class="mt-0.5"
          :data-testid="`extension-model-pick-${model.name}`"
        />
        <span class="flex flex-col">
          <span>{{ model.name }}</span>
          <span class="text-xs text-muted-foreground">
            {{ model.provider }} · {{ t(placeLabelKey(model.place)) }}
          </span>
          <span v-if="model.agent" class="text-xs text-warning">
            {{ t('extensions.permissionRequest.model.agent') }}
          </span>
        </span>
      </label>
    </ShadcnRadioGroup>
    <UiSelect
      v-if="offered.length > 0"
      id="extension-model-effort"
      v-model="effort"
      :options="effortOptions"
      :label="t('extensions.permissionRequest.model.effort')"
      data-testid="extension-model-effort"
    />
    <p v-if="offered.length > 0" class="text-xs text-muted-foreground">
      {{ t('extensions.permissionRequest.model.effortHint') }}
    </p>
  </div>
</template>
