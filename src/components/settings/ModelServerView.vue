<script setup lang="ts">
/**
 * One model server (spec 055 US5, T056): its models by kind, and per model what its `/models`
 * answer does not say (FR-019): whether it takes images and which effort levels it knows, as the
 * user states it. What the server does say (spec 012 FR-024) is shown as it is and needs no
 * choice; the choice stays for a server that is silent. A statement holds through every later read
 * of the models; "Nicht angegeben" and an empty field withdraw it. "Modelle neu lesen" asks the
 * server again; "Entfernen" removes the server. Writes run the catalog actions.
 */
import { toast } from 'vue-sonner'
import type { Provider, ProviderModel } from '~/composables/useProviders'
import type { SettingsSelectOption } from '~/components/settings/Select.vue'
import {
  acceptsImagesOf,
  groupByKind,
  imagesChoice,
  imagesReported,
  kindOf,
  LEVEL_SUGGESTION,
  levelsText,
  parseLevels,
  showsLevels,
  suggestsLevels,
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
const refreshing = ref(false)
const confirmRemove = ref(false)

const groups = computed(() => groupByKind(models.value))

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
    toast.error(errString(error))
  } finally {
    loaded.value = true
  }
}

async function runAsync(action: () => Promise<unknown>) {
  try {
    await action()
  } catch (error) {
    toast.error(errString(error))
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

/** One click takes the common levels for a model that reasons; the field stays editable. */
function suggestLevelsAsync(model: ProviderModel) {
  levels.value[model.id] = levelsText(LEVEL_SUGGESTION)
  return setLevelsAsync(model)
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
  try {
    await removeServer({ providerId: server.value.id })
    await router.push('/ai/providers')
  } catch (error) {
    toast.error(errString(error))
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
      <SettingsGroup
        v-if="models.length === 0"
        :label="t('settings.modelServers.modelsTitle')"
      >
        <li class="px-4 py-3 text-sm text-muted-foreground">
          {{ t('settings.modelServers.noModels') }}
        </li>
      </SettingsGroup>
      <SettingsGroup
        v-for="group in groups"
        :key="group.kind"
        :label="t(`settings.modelServers.kind.${group.kind}`)"
        :data-testid="`model-server-kind-${group.kind}`"
      >
        <SettingsRow
          v-for="model in group.models"
          :key="model.id"
          :title="model.name"
          :data-model="model.name"
        >
          <template v-if="kindOf(model.capabilities) === 'text'">
            <span
              v-if="imagesReported(model.capabilities) !== null"
              class="text-sm text-muted-foreground"
              :data-testid="`model-server-images-${model.name}`"
            >
              {{
                t(
                  imagesReported(model.capabilities)
                    ? 'settings.modelServers.imagesReportedYes'
                    : 'settings.modelServers.imagesReportedNo',
                )
              }}
            </span>
            <SettingsSelect
              v-else
              :model-value="imagesChoice(model.capabilities?.user)"
              :options="imagesOptions"
              :aria-label="
                t('settings.modelServers.imagesOf', { name: model.name })
              "
              class="w-48"
              :data-testid="`model-server-images-${model.name}`"
              @update:model-value="setImagesAsync(model, $event)"
            />
          </template>
          <template v-if="showsLevels(model.capabilities)" #below>
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
              <UiButton
                v-else-if="suggestsLevels(model.capabilities)"
                type="button"
                variant="outline"
                :data-testid="`model-server-suggest-levels-${model.name}`"
                @click="suggestLevelsAsync(model)"
              >
                {{ levelsText(LEVEL_SUGGESTION) }}
              </UiButton>
            </div>
          </template>
        </SettingsRow>
      </SettingsGroup>

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
