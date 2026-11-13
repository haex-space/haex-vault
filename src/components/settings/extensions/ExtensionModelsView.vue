<script setup lang="ts">
/**
 * The models one extension may use (spec 055 US2, T037): every model of the vault, one accordion
 * per provider so a long list stays short. The select at the top chooses the extension's default
 * model among those it may use: its jobs go to that one, and when it cannot take a job holzi asks
 * which other model should. The select of a model sets the highest effort ("Nicht erlaubt" takes
 * the model away again, the next job asks); the select of a provider allows or takes away all its
 * models at once, each with its own starting effort, since the models do not offer the same
 * efforts. A change holds from the extension's next job. A model the user refused or allowed for
 * this vault session only says so.
 */
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'vue-sonner'
import type { ExtensionModelEntry } from '@bindings/ExtensionModelEntry'
import type { SettingsSelectOption } from '~/components/settings/Select.vue'
import {
  allowGroup,
  groupChoice,
  groupModelsByProvider,
  type ProviderModels,
} from '~/lib/extensions/modelGrants'

const props = defineProps<{ extensionId: string }>()
const { t } = useI18n()
const { errString } = useErrorString()

const models = ref<ExtensionModelEntry[]>([])
const busy = ref(false)
/** The provider whose "allow all" waits for the user to confirm the agents in it. */
const confirming = ref<string | null>(null)

const groups = computed(() => groupModelsByProvider(models.value))
const allowed = computed(() =>
  models.value.filter((model) => model.ceiling !== undefined),
)
const defaultId = computed(
  () => allowed.value.find((model) => model.isDefault)?.id ?? '',
)
const defaultGroups = computed(() =>
  groupModelsByProvider(allowed.value).map((group) => ({
    key: group.key,
    label: labelOf(group),
    options: group.items.map((model) => ({
      value: model.id,
      label: model.name,
    })),
  })),
)
const noDefault = computed<SettingsSelectOption[]>(() => [
  { value: '', label: t('settings.extensions.models.noDefault') },
])
const groupActions = computed<SettingsSelectOption[]>(() => [
  { value: '', label: t('settings.extensions.models.groupChoose') },
  { value: 'allow', label: t('settings.extensions.models.allowAll') },
  { value: 'revoke', label: t('settings.extensions.models.revokeAll') },
])

function labelOf(group: ProviderModels<ExtensionModelEntry>): string {
  return group.provider ?? t('agentProfiles.models.local')
}

async function loadAsync() {
  try {
    models.value = await invoke<ExtensionModelEntry[]>(
      'extension_model_grants_list',
      { extensionId: props.extensionId },
    )
  } catch (error) {
    toast.error(errString(error))
  }
}

async function runAsync(action: () => Promise<unknown>) {
  busy.value = true
  try {
    await action()
  } catch (error) {
    toast.error(errString(error))
  }
  await loadAsync()
  busy.value = false
}

function setCeilingAsync(model: ExtensionModelEntry, effort: string) {
  const args = { extensionId: props.extensionId, modelId: model.id }
  return runAsync(() =>
    effort === ''
      ? invoke('extension_model_grants_revoke', { args })
      : invoke('extension_model_grants_set', { args: { ...args, effort } }),
  )
}

function setDefaultAsync(modelId: string) {
  return runAsync(() =>
    invoke('extension_model_default_set', {
      args: { extensionId: props.extensionId, modelId: modelId || null },
    }),
  )
}

function allowAllAsync(group: ProviderModels<ExtensionModelEntry>) {
  confirming.value = null
  return runAsync(async () => {
    for (const { modelId, effort } of allowGroup(group.items)) {
      await invoke('extension_model_grants_set', {
        args: { extensionId: props.extensionId, modelId, effort },
      })
    }
  })
}

function revokeAllAsync(group: ProviderModels<ExtensionModelEntry>) {
  confirming.value = null
  return runAsync(async () => {
    for (const model of group.items.filter((m) => m.ceiling !== undefined)) {
      await invoke('extension_model_grants_revoke', {
        args: { extensionId: props.extensionId, modelId: model.id },
      })
    }
  })
}

/** "Alle erlauben" grants agents with access to files and commands, so it asks first. */
function onGroupAction(
  group: ProviderModels<ExtensionModelEntry>,
  action: string,
) {
  if (action === 'revoke') void revokeAllAsync(group)
  else if (action === 'allow') {
    if (groupChoice(group.items).agents > 0) confirming.value = group.key
    else void allowAllAsync(group)
  }
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
    <SettingsRow
      v-if="allowed.length > 0"
      :title="t('settings.extensions.models.default')"
      :description="t('settings.extensions.models.defaultHint')"
      label-for="extension-model-default"
    >
      <SettingsSelect
        id="extension-model-default"
        :model-value="defaultId"
        :options="noDefault"
        :groups="defaultGroups"
        :disabled="busy"
        class="w-56"
        data-testid="extension-model-default"
        @update:model-value="setDefaultAsync"
      />
    </SettingsRow>
    <AgentsProviderGroup
      v-for="group in groups"
      :key="group.key"
      :label="labelOf(group)"
      :count="group.items.length"
    >
      <SettingsRow
        :title="t('settings.extensions.models.groupAll')"
        :description="
          t('settings.extensions.models.groupState', {
            allowed: groupChoice(group.items).allowed,
            total: groupChoice(group.items).total,
          })
        "
        :data-testid="`extension-model-group-${group.key}`"
      >
        <SettingsSelect
          model-value=""
          :options="groupActions"
          :disabled="busy"
          class="w-56"
          :aria-label="t('settings.extensions.models.groupAll')"
          :data-testid="`extension-model-group-action-${group.key}`"
          @update:model-value="onGroupAction(group, $event)"
        />
      </SettingsRow>
      <SettingsRow
        v-if="confirming === group.key"
        :title="
          t('settings.extensions.models.confirmAgents', {
            count: groupChoice(group.items).agents,
          })
        "
        :description="t('settings.extensions.models.confirmAgentsHint')"
        icon="lucide:triangle-alert"
      >
        <UiButton size="sm" variant="outline" @click="confirming = null">
          {{ t('settings.extensions.models.confirmCancel') }}
        </UiButton>
        <UiButton
          size="sm"
          :disabled="busy"
          data-testid="extension-model-group-confirm"
          @click="allowAllAsync(group)"
        >
          {{ t('settings.extensions.models.allowAll') }}
        </UiButton>
      </SettingsRow>
      <ExtensionModelRow
        v-for="model in group.items"
        :key="model.id"
        :model="model"
        @ceiling="setCeilingAsync(model, $event)"
      />
    </AgentsProviderGroup>
  </SettingsGroup>
</template>
