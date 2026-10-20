<script setup lang="ts">
/**
 * "Modell-Server" (spec 055 US5, T056): OpenAI-compatible servers such as a GPU machine, a school
 * server or a cloud API, each with its address and an optional key. A row leads to the server's
 * models; below the list a form adds one, and holzi reads its models at once. A failed read keeps
 * the server and says why here; with a wrong key the user removes it and adds it again. Writes run
 * the catalog actions.
 */
import type { Provider } from '~/composables/useProviders'
import { addressProblem, OPENAI_COMPATIBLE } from '~/lib/models/servers'

const { t } = useI18n()
const { errString } = useErrorString()
const fieldLabels = useFieldLabels()
const router = useTabRouter()
const { listAsync, listModelsAsync } = useProviders()
const addServer = useActionOrThrow('settings.servers.add')

const servers = ref<Provider[]>([])
const modelCounts = ref<Record<string, number>>({})
const loadError = ref<string | null>(null)

const name = ref('')
const address = ref('')
const key = ref('')
const busy = ref(false)
const addError = ref<string | null>(null)
const readFailure = ref<{ name: string; reason: string } | null>(null)
const tried = ref(false)

const problem = computed(() => addressProblem(address.value))
const canAdd = computed(
  () => name.value.trim() !== '' && problem.value === null && !busy.value,
)

async function reloadAsync() {
  loadError.value = null
  try {
    const all = await listAsync()
    servers.value = all.filter((p) => p.adapter === OPENAI_COMPATIBLE)
    const counts: Record<string, number> = {}
    await Promise.all(
      servers.value.map(async (server) => {
        counts[server.id] = (await listModelsAsync(server.id)).length
      }),
    )
    modelCounts.value = counts
  } catch (error) {
    loadError.value = errString(error)
  }
}

function description(server: Provider): string {
  return [
    server.baseUrl ?? '',
    t('settings.modelServers.models', modelCounts.value[server.id] ?? 0),
  ].join(' · ')
}

async function addAsync() {
  tried.value = true
  if (!canAdd.value) return
  busy.value = true
  addError.value = null
  readFailure.value = null
  const addedName = name.value.trim()
  try {
    const added = (await addServer({
      name: addedName,
      baseUrl: address.value.trim(),
      ...(key.value.trim() === '' ? {} : { apiKey: key.value.trim() }),
    })) as { providerId: string; refreshError?: string }
    name.value = ''
    address.value = ''
    key.value = ''
    tried.value = false
    if (added.refreshError !== undefined) {
      readFailure.value = { name: addedName, reason: added.refreshError }
      await reloadAsync()
    } else {
      await router.push(`/ai/servers/${added.providerId}`)
    }
  } catch (error) {
    addError.value = errString(error)
  } finally {
    busy.value = false
  }
}

onMounted(reloadAsync)
onVaultTablesChanged(['providers', 'models'], reloadAsync)
</script>

<template>
  <section class="flex flex-col gap-3" data-testid="model-servers">
    <p class="px-1 text-sm text-muted-foreground">
      {{ t('settings.modelServers.intro') }}
    </p>
    <p v-if="loadError" class="px-1 text-sm text-destructive" role="alert">
      {{ loadError }}
    </p>
    <p
      v-if="readFailure"
      class="rounded-xl bg-destructive/10 px-3 py-2 text-sm text-destructive"
      role="alert"
      data-testid="model-server-read-failed"
    >
      {{ t('settings.modelServers.readFailed', readFailure) }}
    </p>

    <SettingsGroup v-if="servers.length > 0">
      <SettingsRow
        v-for="server in servers"
        :key="server.id"
        icon="lucide:server"
        :title="server.name"
        :description="description(server)"
        :to="`/ai/servers/${server.id}`"
        data-testid="model-server-row"
      />
    </SettingsGroup>

    <SettingsGroup :label="t('settings.modelServers.add')">
      <div class="flex flex-col gap-4 p-4">
        <UiInput
          id="model-server-name"
          v-model="name"
          :label="t('settings.modelServers.name')"
          :labels="fieldLabels.input.value"
          label-bg="var(--muted)"
          maxlength="80"
          data-testid="model-server-name"
        />
        <div class="flex flex-col gap-1">
          <UiInput
            id="model-server-address"
            v-model="address"
            :label="t('settings.modelServers.address')"
            :labels="fieldLabels.input.value"
            label-bg="var(--muted)"
            type="url"
            autocomplete="off"
            placeholder="http://192.168.1.20:11434/v1"
            data-testid="model-server-address"
          />
          <p
            v-if="tried && problem"
            class="text-xs text-destructive"
            role="alert"
          >
            {{ t(`settings.modelServers.addressProblem.${problem}`) }}
          </p>
          <p v-else class="text-xs text-muted-foreground">
            {{ t('settings.modelServers.addressHint') }}
          </p>
        </div>
        <div class="flex flex-col gap-1">
          <UiInputPassword
            id="model-server-key"
            v-model="key"
            :label="t('settings.modelServers.key')"
            :labels="fieldLabels.password.value"
            autocomplete="off"
            data-testid="model-server-key"
          />
          <p class="text-xs text-muted-foreground">
            {{ t('settings.modelServers.keyHint') }}
          </p>
        </div>
        <p v-if="addError" class="text-sm text-destructive" role="alert">
          {{ addError }}
        </p>
        <div class="flex justify-end">
          <UiButton
            type="button"
            :disabled="busy"
            data-testid="model-server-add"
            @click="addAsync"
          >
            {{
              busy
                ? t('settings.modelServers.adding')
                : t('settings.modelServers.addButton')
            }}
          </UiButton>
        </div>
      </div>
    </SettingsGroup>
  </section>
</template>
