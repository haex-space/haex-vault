<script setup lang="ts">
/**
 * One model server (spec 055 US5, T056): its models, and per model what its `/models` answer does
 * not say (FR-019): whether it takes images and which effort levels it knows, as the user states
 * it. A statement holds through every later read of the models; "Nicht angegeben" and an empty
 * field withdraw it. "Modelle neu lesen" asks the server again; "Entfernen" removes the server.
 * Writes run the catalog actions.
 */
import type { Provider, ProviderModel } from '~/composables/useProviders'
import type { SettingsSelectOption } from '~/components/settings/Select.vue'
import {
  acceptsImagesOf,
  imagesChoice,
  levelsText,
  parseLevels,
  OPENAI_COMPATIBLE,
  type ImagesChoice,
} from '~/lib/models/servers'

const { t } = useI18n()
const { errString } = useErrorString()
const fieldLabels = useFieldLabels()
const router = useTabRouter()
const { listAsync, listModelsAsync } = useProviders()
const refreshModels = useActionOrThrow('settings.delegate.refreshModels')
const removeServer = useActionOrThrow('settings.servers.remove')
const setCapabilities = useActionOrThrow(
  'settings.servers.setModelCapabilities',
)

const providerId = computed(() => String(router.route.params.providerId ?? ''))
const server = ref<Provider | null>(null)
const models = ref<ProviderModel[]>([])
const levels = ref<Record<string, string>>({})
const loaded = ref(false)
const failure = ref<string | null>(null)
const refreshing = ref(false)
const confirmRemove = ref(false)

const imagesOptions = computed<SettingsSelectOption[]>(() =>
  (['unstated', 'yes', 'no'] as const).map((value) => ({
    value,
    label: t(`settings.modelServers.images.${value}`),
  })),
)

async function reloadAsync() {
  try {
    server.value =
      (await listAsync()).find(
        (p) => p.id === providerId.value && p.adapter === OPENAI_COMPATIBLE,
      ) ?? null
    models.value = server.value ? await listModelsAsync(server.value.id) : []
    levels.value = Object.fromEntries(
      models.value.map((m) => [
        m.id,
        levelsText(m.capabilities?.user?.effortLevels),
      ]),
    )
  } catch (error) {
    failure.value = errString(error)
  } finally {
    loaded.value = true
  }
}

async function runAsync(action: () => Promise<unknown>) {
  failure.value = null
  try {
    await action()
  } catch (error) {
    failure.value = errString(error)
  }
  await reloadAsync()
}

/** Stores both statements of a model at once: a field left out is not stated. */
function stateAsync(model: ProviderModel, images: ImagesChoice, text: string) {
  const acceptsImages = acceptsImagesOf(images)
  const effortLevels = parseLevels(text)
  return runAsync(() =>
    setCapabilities({
      modelId: model.id,
      ...(acceptsImages === null ? {} : { acceptsImages }),
      ...(effortLevels === null ? {} : { effortLevels }),
    }),
  )
}

function setImagesAsync(model: ProviderModel, choice: string) {
  return stateAsync(
    model,
    choice as ImagesChoice,
    levelsText(model.capabilities?.user?.effortLevels),
  )
}

function setLevelsAsync(model: ProviderModel) {
  return stateAsync(
    model,
    imagesChoice(model.capabilities?.user),
    levels.value[model.id] ?? '',
  )
}

function levelsChanged(model: ProviderModel): boolean {
  return (
    (levels.value[model.id] ?? '') !==
    levelsText(model.capabilities?.user?.effortLevels)
  )
}

async function refreshAsync() {
  if (!server.value) return
  refreshing.value = true
  await runAsync(() => refreshModels({ providerId: server.value!.id }))
  refreshing.value = false
}

async function removeAsync() {
  if (!server.value) return
  failure.value = null
  try {
    await removeServer({ providerId: server.value.id })
    await router.push('/ai/servers')
  } catch (error) {
    failure.value = errString(error)
  }
}

onMounted(reloadAsync)
onVaultTablesChanged(['providers', 'models'], reloadAsync)
</script>

<template>
  <section class="flex flex-col gap-3" data-testid="model-server">
    <p
      v-if="loaded && !server"
      class="px-1 text-sm text-muted-foreground"
      data-testid="model-server-missing"
    >
      {{ t('settings.modelServers.missing') }}
    </p>
    <template v-if="server">
      <SettingsGroup :label="server.name">
        <SettingsRow
          icon="lucide:server"
          :title="server.baseUrl ?? ''"
          :description="
            server.hasCredentials
              ? t('settings.modelServers.withKey')
              : t('settings.modelServers.withoutKey')
          "
        >
          <UiButton
            type="button"
            variant="outline"
            :disabled="refreshing"
            data-testid="model-server-refresh"
            @click="refreshAsync"
          >
            {{
              refreshing
                ? t('settings.modelServers.refreshing')
                : t('settings.modelServers.refresh')
            }}
          </UiButton>
        </SettingsRow>
      </SettingsGroup>

      <p class="px-1 text-sm text-muted-foreground">
        {{ t('settings.modelServers.statementsIntro') }}
      </p>
      <SettingsGroup :label="t('settings.modelServers.modelsTitle')">
        <li
          v-if="models.length === 0"
          class="px-4 py-3 text-sm text-muted-foreground"
        >
          {{ t('settings.modelServers.noModels') }}
        </li>
        <SettingsRow
          v-for="model in models"
          :key="model.id"
          :title="model.name"
          :data-model="model.name"
        >
          <SettingsSelect
            :model-value="imagesChoice(model.capabilities?.user)"
            :options="imagesOptions"
            :aria-label="
              t('settings.modelServers.imagesOf', { name: model.name })
            "
            class="w-48"
            :data-testid="`model-server-images-${model.name}`"
            @update:model-value="setImagesAsync(model, $event)"
          />
          <template #below>
            <div class="flex flex-wrap items-end gap-2">
              <div class="min-w-48 flex-1">
                <UiInput
                  :id="`model-server-levels-${model.id}`"
                  v-model="levels[model.id]"
                  :label="t('settings.modelServers.levels')"
                  :labels="fieldLabels.input.value"
                  label-bg="var(--muted)"
                  autocomplete="off"
                  placeholder="low, medium, high"
                  :data-testid="`model-server-levels-${model.name}`"
                  @keyup.enter="setLevelsAsync(model)"
                />
              </div>
              <UiButton
                v-if="levelsChanged(model)"
                type="button"
                variant="outline"
                @click="setLevelsAsync(model)"
              >
                {{ t('settings.modelServers.saveLevels') }}
              </UiButton>
            </div>
          </template>
        </SettingsRow>
      </SettingsGroup>

      <p v-if="failure" class="px-1 text-sm text-destructive" role="alert">
        {{ failure }}
      </p>

      <SettingsGroup>
        <div class="flex flex-wrap items-center justify-end gap-2 p-4">
          <template v-if="confirmRemove">
            <span class="text-sm">{{
              t('settings.modelServers.removeConfirm')
            }}</span>
            <UiButton
              type="button"
              variant="outline"
              @click="confirmRemove = false"
            >
              {{ t('settings.modelServers.cancel') }}
            </UiButton>
            <UiButton
              type="button"
              variant="destructive"
              data-testid="model-server-remove-confirm"
              @click="removeAsync"
            >
              {{ t('settings.modelServers.remove') }}
            </UiButton>
          </template>
          <UiButton
            v-else
            type="button"
            variant="outline"
            data-testid="model-server-remove"
            @click="confirmRemove = true"
          >
            {{ t('settings.modelServers.remove') }}
          </UiButton>
        </div>
      </SettingsGroup>
    </template>
  </section>
</template>
