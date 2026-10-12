<script setup lang="ts">
/**
 * A storage connection (spec 038, US1, T028, FR-001–FR-005): provider, endpoint, region,
 * addressing and the credentials. The provider presets prefill endpoint and addressing; an `http`
 * endpoint is marked unencrypted. Unlike other settings this form is confirmed once ("Testen und
 * speichern"), because holzi tests a new connection, new credentials or a moved endpoint before it
 * saves anything (FR-003); a failed test saves nothing and names the reason. A new connection
 * also gets its first storage on the tested bucket. The credentials go to the password manager
 * ("S3: <Name>") and never come back to this window; to change them the user types new ones.
 */
import { invoke } from '@tauri-apps/api/core'
import type { Addressing } from '@bindings/Addressing'
import type { ConnectionInput } from '@bindings/ConnectionInput'
import type { ConnectionView } from '@bindings/ConnectionView'
import type { ProviderKind } from '@bindings/ProviderKind'
import type { StorageInput } from '@bindings/StorageInput'
import type { SettingsSelectOption } from '~/components/settings/Select.vue'

const { t } = useI18n()
const router = useTabRouter()
const fieldLabels = useFieldLabels()
const storageError = useStorageError()
const { overview, connection } = useStorageOverview()

const connectionId = computed(() => router.route.params.connectionId ?? 'new')
const isNew = computed(() => connectionId.value === 'new')
const existing = computed(() =>
  isNew.value ? undefined : connection(connectionId.value),
)
const missing = computed(
  () => !isNew.value && overview.value !== null && !existing.value,
)

/** The providers this form offers so far: the S3 ones (WebDAV and SFTP follow, spec 050). */
type S3Kind = Extract<ProviderKind, 'aws' | 'rustfs' | 'other'>

const PRESETS: Record<
  S3Kind,
  { endpoint: string; addressing: Addressing; region: string }
> = {
  aws: { endpoint: '', addressing: 'virtual', region: 'eu-central-1' },
  rustfs: {
    endpoint: 'http://127.0.0.1:9000',
    addressing: 'path',
    region: 'us-east-1',
  },
  other: { endpoint: 'https://', addressing: 'path', region: 'us-east-1' },
}

const draft = reactive({
  providerKind: 'rustfs' as ProviderKind,
  providerName: t('settings.storage.kinds.rustfs'),
  endpoint: PRESETS.rustfs.endpoint,
  region: PRESETS.rustfs.region,
  addressing: PRESETS.rustfs.addressing,
  accessKeyId: '',
  secretAccessKey: '',
  sessionToken: '',
  bucket: '',
  storageName: '',
})
const replaceCredentials = ref(false)
const busy = ref(false)
const failure = ref<string | null>(null)

/** The form shows the stored connection once it is known; later reloads keep what is typed. */
let filled = false
watchEffect(() => {
  const stored = existing.value
  if (filled || !stored) return
  filled = true
  draft.providerKind = stored.providerKind
  draft.providerName = stored.providerName
  draft.endpoint = stored.endpoint
  draft.region = stored.region
  draft.addressing = stored.addressing
  draft.bucket =
    overview.value?.storages.find((s) => s.connectionId === stored.id)
      ?.bucket ?? ''
  replaceCredentials.value = stored.credentials === 'missing'
})

const kindOptions = computed<SettingsSelectOption[]>(() =>
  (['rustfs', 'aws', 'other'] as const).map((kind) => ({
    value: kind,
    label: t(`settings.storage.kinds.${kind}`),
  })),
)
const addressingOptions = computed<SettingsSelectOption[]>(() =>
  (['path', 'virtual'] as const).map((mode) => ({
    value: mode,
    label: t(`settings.storage.addressingModes.${mode}`),
  })),
)

/** A new provider prefills its endpoint, region and addressing, and the name while untouched. */
function choose(kind: string) {
  const previous = draft.providerKind
  const next = kind as S3Kind
  draft.providerKind = next
  Object.assign(draft, PRESETS[next])
  if (draft.providerName === t(`settings.storage.kinds.${previous}`))
    draft.providerName = t(`settings.storage.kinds.${next}`)
}

const insecure = computed(() =>
  draft.endpoint.trim().toLowerCase().startsWith('http://'),
)
const withCredentials = computed(() => isNew.value || replaceCredentials.value)

function clearSecrets() {
  draft.secretAccessKey = ''
  draft.sessionToken = ''
}

async function saveAsync() {
  if (busy.value) return
  busy.value = true
  failure.value = null
  const input: ConnectionInput = {
    id: isNew.value ? undefined : connectionId.value,
    protocol: 's3',
    providerName: draft.providerName,
    providerKind: draft.providerKind,
    endpoint: draft.endpoint.trim() || undefined,
    region: draft.region,
    addressing: draft.addressing,
    credentials: withCredentials.value
      ? {
          kind: 's3',
          accessKeyId: draft.accessKeyId,
          secretAccessKey: draft.secretAccessKey,
          sessionToken: draft.sessionToken.trim() || undefined,
        }
      : undefined,
    bucketForTest: draft.bucket.trim(),
  }
  try {
    const saved = await invoke<ConnectionView>('storage_connection_save', {
      input,
    })
    clearSecrets()
    if (isNew.value) {
      const storage: StorageInput = {
        connectionId: saved.id,
        name: draft.storageName.trim() || draft.bucket.trim(),
        bucket: draft.bucket.trim(),
      }
      try {
        await invoke('storage_save', { input: storage })
      } catch (error) {
        // The connection stands; the storage can be added from the list.
        router.replace(`/storage/connections/${saved.id}`)
        failure.value = storageError(error)
        return
      }
    }
    router.back()
  } catch (error) {
    failure.value = storageError(error)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <section class="flex flex-col gap-3" data-testid="storage-connection-form">
    <p v-if="missing" class="px-1 text-sm text-muted-foreground" role="status">
      {{ t('settings.storage.notFound') }}
    </p>

    <form v-else class="flex flex-col gap-3" @submit.prevent="saveAsync">
      <SettingsGroup :label="t('settings.storage.groups.connection')">
        <SettingsRow
          :title="t('settings.storage.fields.providerKind')"
          label-for="storage-provider-kind"
        >
          <SettingsSelect
            id="storage-provider-kind"
            :model-value="draft.providerKind"
            :options="kindOptions"
            data-testid="storage-provider-kind"
            @update:model-value="choose"
          />
        </SettingsRow>
        <li class="px-4 py-3">
          <UiInput
            id="storage-provider-name"
            v-model="draft.providerName"
            :label="t('settings.storage.fields.providerName')"
            :labels="fieldLabels.input.value"
            label-bg="var(--muted)"
            maxlength="80"
            data-testid="storage-provider-name"
          />
        </li>
        <li
          v-if="draft.providerKind !== 'aws'"
          class="flex flex-col gap-2 px-4 py-3"
        >
          <UiInput
            id="storage-endpoint"
            v-model="draft.endpoint"
            :label="t('settings.storage.fields.endpoint')"
            :labels="fieldLabels.input.value"
            label-bg="var(--muted)"
            type="url"
            autocomplete="off"
            data-testid="storage-endpoint"
          />
          <p
            v-if="insecure"
            class="text-sm text-destructive"
            data-testid="storage-endpoint-insecure"
          >
            {{ t('settings.storage.insecureHint') }}
          </p>
        </li>
        <li class="px-4 py-3">
          <UiInput
            id="storage-region"
            v-model="draft.region"
            :label="t('settings.storage.fields.region')"
            :labels="fieldLabels.input.value"
            label-bg="var(--muted)"
            maxlength="64"
            autocomplete="off"
            data-testid="storage-region"
          />
        </li>
        <SettingsRow
          :title="t('settings.storage.fields.addressing')"
          label-for="storage-addressing"
        >
          <SettingsSelect
            id="storage-addressing"
            v-model="draft.addressing"
            :options="addressingOptions"
            data-testid="storage-addressing"
          />
        </SettingsRow>
      </SettingsGroup>

      <SettingsGroup :label="t('settings.storage.groups.credentials')">
        <SettingsOptionRow
          v-if="!isNew"
          type="checkbox"
          :checked="replaceCredentials"
          :title="t('settings.storage.newCredentials')"
          :description="
            t('settings.storage.credentialsKept', {
              name: existing?.providerName ?? draft.providerName,
            })
          "
          data-testid="storage-replace-credentials"
          @change="replaceCredentials = $event"
        />
        <template v-if="withCredentials">
          <li class="px-4 py-3">
            <UiInput
              id="storage-access-key"
              v-model="draft.accessKeyId"
              :label="t('settings.storage.fields.accessKeyId')"
              :labels="fieldLabels.input.value"
              label-bg="var(--muted)"
              autocomplete="off"
              data-testid="storage-access-key"
            />
          </li>
          <li class="px-4 py-3">
            <UiInputPassword
              id="storage-secret"
              v-model="draft.secretAccessKey"
              :label="t('settings.storage.fields.secretAccessKey')"
              :labels="fieldLabels.password.value"
              label-bg="var(--muted)"
              autocomplete="new-password"
              data-testid="storage-secret"
            />
          </li>
          <li class="px-4 py-3">
            <UiInputPassword
              id="storage-session-token"
              v-model="draft.sessionToken"
              :label="t('settings.storage.fields.sessionToken')"
              :labels="fieldLabels.password.value"
              label-bg="var(--muted)"
              autocomplete="off"
              data-testid="storage-session-token"
            />
          </li>
        </template>
      </SettingsGroup>

      <SettingsGroup
        :label="
          isNew
            ? t('settings.storage.groups.storage')
            : t('settings.storage.groups.test')
        "
      >
        <li class="flex flex-col gap-2 px-4 py-3">
          <UiInput
            id="storage-bucket"
            v-model="draft.bucket"
            :label="t('settings.storage.fields.bucket')"
            :labels="fieldLabels.input.value"
            label-bg="var(--muted)"
            autocomplete="off"
            data-testid="storage-bucket"
          />
          <p class="text-sm text-muted-foreground">
            {{ t('settings.storage.testBucketHint') }}
          </p>
        </li>
        <li v-if="isNew" class="px-4 py-3">
          <UiInput
            id="storage-name"
            v-model="draft.storageName"
            :label="t('settings.storage.fields.storageName')"
            :placeholder="draft.bucket"
            :labels="fieldLabels.input.value"
            label-bg="var(--muted)"
            maxlength="80"
            data-testid="storage-name"
          />
        </li>
      </SettingsGroup>

      <p
        v-if="failure"
        class="px-1 text-sm text-destructive"
        role="alert"
        data-testid="storage-failure"
      >
        {{ failure }}
      </p>
      <div class="flex flex-wrap gap-2">
        <UiButton type="submit" :loading="busy" data-testid="storage-save">
          {{
            busy ? t('settings.storage.testing') : t('settings.storage.save')
          }}
        </UiButton>
        <UiButton
          v-if="!isNew"
          type="button"
          variant="outline"
          :disabled="busy"
          data-testid="storage-remove-connection"
          @click="router.push(`/storage/remove/connection/${connectionId}`)"
        >
          {{ t('settings.storage.remove') }}
        </UiButton>
      </div>
    </form>
  </section>
</template>
