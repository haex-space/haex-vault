<script setup lang="ts">
/**
 * The models one extension may use (spec 055 US2, T037): every model of the vault, those the
 * extension may use first in its rank. The select sets the highest effort ("Nicht erlaubt" takes
 * the model away again, the next job asks); the arrows change the rank, by which holzi picks a
 * model for a job. A change holds from the extension's next job. A model the user refused or
 * allowed for this vault session only says so.
 */
import { invoke } from '@tauri-apps/api/core'
import type { ExtensionModelEntry } from '@bindings/ExtensionModelEntry'
import type { SettingsSelectOption } from '~/components/settings/Select.vue'
import {
  ceilingChoices,
  effortLabelKey,
  moved,
  placeLabelKey,
  rankOf,
} from '~/lib/extensions/modelGrants'

const props = defineProps<{ extensionId: string }>()
const { t } = useI18n()
const { errString } = useErrorString()

const models = ref<ExtensionModelEntry[]>([])
const failure = ref<string | null>(null)
const rank = computed(() => rankOf(models.value))

/**
 * The efforts of one model: "Nicht erlaubt" (`''`) and the efforts the model offers. A model
 * without a choice of effort has only `none`, which says "Erlaubt".
 */
function ceilingOptions(model: ExtensionModelEntry): SettingsSelectOption[] {
  return [
    { value: '', label: t('settings.extensions.models.notAllowed') },
    ...ceilingChoices(model.efforts, model.ceiling).map((value) => ({
      value,
      label:
        model.efforts.length === 0
          ? t('settings.extensions.models.allowed')
          : t(effortLabelKey(value)),
    })),
  ]
}

async function loadAsync() {
  try {
    models.value = await invoke<ExtensionModelEntry[]>(
      'extension_model_grants_list',
      { extensionId: props.extensionId },
    )
  } catch (error) {
    failure.value = errString(error)
  }
}

async function runAsync(action: () => Promise<unknown>) {
  failure.value = null
  try {
    await action()
  } catch (error) {
    failure.value = errString(error)
  }
  await loadAsync()
}

function setCeilingAsync(model: ExtensionModelEntry, effort: string) {
  const args = { extensionId: props.extensionId, modelId: model.id }
  return runAsync(() =>
    effort === ''
      ? invoke('extension_model_grants_revoke', { args })
      : invoke('extension_model_grants_set', { args: { ...args, effort } }),
  )
}

function moveAsync(model: ExtensionModelEntry, delta: -1 | 1) {
  return runAsync(() =>
    invoke('extension_model_grants_reorder', {
      args: {
        extensionId: props.extensionId,
        modelIds: moved(rank.value, model.id, delta),
      },
    }),
  )
}

function detail(model: ExtensionModelEntry): string {
  const parts = [model.provider, t(placeLabelKey(model.place))]
  if (model.takesImages) parts.push(t('settings.extensions.models.takesImages'))
  if (model.agent) parts.push(t('settings.extensions.models.agent'))
  if (model.denied) parts.push(t('settings.extensions.models.denied'))
  if (model.held) parts.push(t('settings.extensions.models.held'))
  return parts.join(' · ')
}

onMounted(loadAsync)
</script>

<template>
  <SettingsGroup
    :label="t('settings.extensions.models.title')"
    data-testid="extension-models"
  >
    <li
      v-if="models.length === 0"
      class="px-4 py-3 text-sm text-muted-foreground"
    >
      {{ t('settings.extensions.models.none') }}
    </li>
    <li
      v-for="model in models"
      :key="model.id"
      class="flex flex-wrap items-center gap-x-4 gap-y-2 px-4 py-3"
      :data-model="model.name"
    >
      <div class="flex min-w-48 flex-1 flex-col">
        <span class="text-sm">{{ model.name }}</span>
        <span class="text-xs text-muted-foreground">{{ detail(model) }}</span>
      </div>
      <template v-if="model.ceiling !== undefined">
        <UiButton
          variant="ghost"
          size="icon"
          :disabled="rank.indexOf(model.id) === 0"
          :aria-label="t('settings.extensions.models.moveUp')"
          :title="t('settings.extensions.models.moveUp')"
          @click="moveAsync(model, -1)"
        >
          <Icon name="lucide:arrow-up" class="h-4 w-4" :aria-hidden="true" />
        </UiButton>
        <UiButton
          variant="ghost"
          size="icon"
          :disabled="rank.indexOf(model.id) === rank.length - 1"
          :aria-label="t('settings.extensions.models.moveDown')"
          :title="t('settings.extensions.models.moveDown')"
          @click="moveAsync(model, 1)"
        >
          <Icon name="lucide:arrow-down" class="h-4 w-4" :aria-hidden="true" />
        </UiButton>
      </template>
      <SettingsSelect
        :model-value="model.ceiling ?? ''"
        :options="ceilingOptions(model)"
        :aria-label="
          t('settings.extensions.models.ceiling', { name: model.name })
        "
        class="w-44"
        :data-testid="`extension-model-ceiling-${model.name}`"
        @update:model-value="setCeilingAsync(model, $event)"
      />
    </li>
    <li v-if="failure" class="px-4 py-3 text-sm text-destructive" role="alert">
      {{ failure }}
    </li>
  </SettingsGroup>
</template>
