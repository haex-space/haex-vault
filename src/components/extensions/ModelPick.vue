<script setup lang="ts">
/**
 * The choice in the question of a job (spec 055 US2, T036): which of the models that fit the job
 * the extension may use, and up to which effort. holzi offers only models that can do what the job
 * needs, the one it proposes first; the user's pick goes back with the answer. The models are one
 * select grouped by provider, so a subscription with a hundred models stays usable.
 */
import {
  ceilingChoices,
  effortLabelKey,
  groupModelsByProvider,
  placeLabelKey,
} from '~/lib/extensions/modelGrants'

interface PickModel {
  id: string
  name: string
  provider: string
  place: string
  agent?: boolean
  efforts: string[]
}

const props = defineProps<{ models: PickModel[] }>()
const modelId = defineModel<string>('modelId', { required: true })
const effort = defineModel<string>('effort', { required: true })
const { t } = useI18n()

const chosen = computed(() => props.models.find((m) => m.id === modelId.value))
const groups = computed(() =>
  groupModelsByProvider(props.models).map((group) => ({
    key: group.key,
    label: group.provider ?? t('agentProfiles.models.local'),
    options: group.items.map((model) => ({
      value: model.id,
      label: model.name,
    })),
  })),
)

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
    <SettingsSelect
      id="extension-model-pick"
      v-model="modelId"
      :groups="groups"
      class="w-full"
      :aria-label="t('extensions.permissionRequest.model.models')"
      data-testid="extension-model-pick"
    />
    <div v-if="chosen" class="flex flex-col text-xs text-muted-foreground">
      <span>{{ chosen.provider }} · {{ t(placeLabelKey(chosen.place)) }}</span>
      <span v-if="chosen.agent" class="text-warning">
        {{ t('extensions.permissionRequest.model.agent') }}
      </span>
    </div>
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
