<script setup lang="ts">
/**
 * A storage connection (spec 038, US1, T028, FR-001–FR-005; spec 050 US1): protocol, provider,
 * endpoint, and for S3 region and addressing, and the credentials. The protocol is chosen once, for
 * a new connection. The provider presets prefill endpoint and addressing; a WebDAV preset shows the
 * parts in `<…>` the user fills in (spec 050 research R11). An `http` endpoint is marked
 * unencrypted. Unlike other settings this form is confirmed once ("Testen und
 * speichern"), because holzi tests a new connection, new credentials or a moved endpoint before it
 * saves anything (FR-003); a failed test saves nothing and names the reason. A new connection
 * also gets its first storage on the tested bucket or folder; a missing folder is made only when
 * the user says so (FR-006). Typed credentials go to the password manager ("S3: <Name>",
 * "WebDAV: <Name>") and never come back to this window; instead of typing, the user may choose an
 * entry there that fits the protocol (spec 052 US1). To change them the user types new ones or
 * chooses another entry.
 */
import { invoke } from '@tauri-apps/api/core'
import type { Addressing } from '@bindings/Addressing'
import type { ConnectionInput } from '@bindings/ConnectionInput'
import type { ConnectionView } from '@bindings/ConnectionView'
import type { Protocol } from '@bindings/Protocol'
import type { ProviderKind } from '@bindings/ProviderKind'
import type { ShownHostKey } from '@bindings/ShownHostKey'
import type { StorageInput } from '@bindings/StorageInput'
import type { SettingsSelectOption } from '~/components/settings/Select.vue'
import CredentialsEntryPicker from './CredentialsEntryPicker.vue'
import CredentialsFields from './CredentialsFields.vue'
import CredentialsLinkedEntry from './CredentialsLinkedEntry.vue'
import CredentialsMissingNote from './CredentialsMissingNote.vue'
import { clearSecrets, credentialsInput, emptyCredentials } from './credentials'
import HostKeyConfirm from './HostKeyConfirm.vue'
import { PRESETS, type FormProtocol } from './presets'

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

const draft = reactive({
  protocol: 's3' as Protocol,
  providerKind: 'rustfs' as ProviderKind,
  providerName: t('settings.storage.kinds.rustfs'),
  endpoint: 'http://127.0.0.1:9000',
  region: 'us-east-1',
  addressing: 'path' as Addressing,
  bucket: '',
  storageName: '',
})
const credentials = ref(emptyCredentials())
/** The host key the server showed, waiting for the user's word (SFTP, research R5). */
const shown = ref<{ key: ShownHostKey; changed: boolean } | null>(null)
/** The host key the user confirmed; the next save sends it. */
const confirmedKey = ref<string | undefined>()
const replaceCredentials = ref(false)
const busy = ref(false)
const failure = ref<string | null>(null)
/** The last test found the folder missing; the user may let holzi make it (FR-006). */
const folderMissing = ref(false)
const tree = computed(() => draft.protocol !== 's3')

/** The form shows the stored connection once it is known; later reloads keep what is typed. */
let filled = false
watchEffect(() => {
  const stored = existing.value
  if (filled || !stored) return
  filled = true
  draft.protocol = stored.protocol
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

const protocolOptions = computed<SettingsSelectOption[]>(() =>
  (['s3', 'webdav', 'sftp'] as const).map((protocol) => ({
    value: protocol,
    label: t(`settings.storage.protocols.${protocol}`),
  })),
)
const kindOptions = computed<SettingsSelectOption[]>(() =>
  Object.keys(PRESETS[draft.protocol as FormProtocol] ?? {}).map((kind) => ({
    value: kind,
    label: t(`settings.storage.kinds.${kind}`),
  })),
)
/** The hint of a provider: what to fill in or switch on at the provider (research R11). */
const presetHint = computed(() => {
  if (draft.providerKind === 'other') return null
  if (draft.protocol === 'webdav')
    return t(`settings.storage.presetHints.${draft.providerKind}`)
  if (draft.protocol === 'sftp')
    return t(`settings.storage.sftpHints.${draft.providerKind}`)
  return null
})
const placeholdersLeft = computed(() => /<[^>]+>/.test(draft.endpoint))
const addressingOptions = computed<SettingsSelectOption[]>(() =>
  (['path', 'virtual'] as const).map((mode) => ({
    value: mode,
    label: t(`settings.storage.addressingModes.${mode}`),
  })),
)

/** A new provider prefills its endpoint, region and addressing, and the name while untouched. */
function choose(kind: string) {
  const previous = draft.providerKind
  const next = kind as ProviderKind
  draft.providerKind = next
  const preset = PRESETS[draft.protocol as FormProtocol]?.[next]
  if (preset) Object.assign(draft, preset)
  if (draft.providerName === t(`settings.storage.kinds.${previous}`))
    draft.providerName = t(`settings.storage.kinds.${next}`)
  folderMissing.value = false
}

/** Another protocol starts from its first provider (only for a new connection, FR-001). */
function chooseProtocol(protocol: string) {
  draft.protocol = protocol as Protocol
  const first = Object.keys(PRESETS[protocol as FormProtocol] ?? {})[0]
  if (first) choose(first)
}

const sourceOptions = computed<SettingsSelectOption[]>(() =>
  (['new', 'entry'] as const).map((source) => ({
    value: source,
    label: t(`settings.storage.credentialsSource.${source}`),
  })),
)

/** The way to new credentials the user picked for a connection without them (spec 052 US3). */
function chooseSource(source: 'entry' | 'new') {
  replaceCredentials.value = true
  credentials.value.source = source
}

const insecure = computed(() =>
  draft.endpoint.trim().toLowerCase().startsWith('http://'),
)
const withCredentials = computed(() => isNew.value || replaceCredentials.value)

/** The credentials as typed, in the kind of the protocol. */
function typedCredentials(): ConnectionInput['credentials'] {
  if (!withCredentials.value) return undefined
  return credentialsInput(draft.protocol, credentials.value)
}

async function saveAsync(createFolder = false) {
  if (busy.value) return
  busy.value = true
  failure.value = null
  folderMissing.value = false
  if (
    withCredentials.value &&
    credentials.value.source === 'entry' &&
    !credentials.value.entryId
  ) {
    failure.value = t('settings.storage.entryPicker.required')
    busy.value = false
    return
  }
  const input: ConnectionInput = {
    id: isNew.value ? undefined : connectionId.value,
    protocol: draft.protocol,
    providerName: draft.providerName,
    providerKind: draft.providerKind,
    endpoint: draft.endpoint.trim() || undefined,
    region: tree.value ? '' : draft.region,
    addressing: tree.value ? 'path' : draft.addressing,
    credentials: typedCredentials(),
    bucketForTest: draft.bucket.trim(),
    createFolder,
    hostKey: confirmedKey.value,
  }
  try {
    const saved = await invoke<ConnectionView>('storage_connection_save', {
      input,
    })
    clearSecrets(credentials.value)
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
    folderMissing.value = tree.value && isFolderMissing(error)
    shown.value = shownHostKey(error)
  } finally {
    busy.value = false
  }
}

/** The user confirmed the shown host key: a new one goes with the next save, a changed one is
 * taken first (only exactly the key the server showed, research R5). */
async function confirmHostKeyAsync() {
  const key = shown.value
  if (!key) return
  shown.value = null
  if (key.changed && !isNew.value) {
    try {
      await invoke('storage_host_key_accept', {
        connectionId: connectionId.value,
        hostKey: key.key.key,
      })
    } catch (error) {
      failure.value = storageError(error)
      return
    }
  } else {
    confirmedKey.value = key.key.key
  }
  await saveAsync()
}
</script>

<template>
  <section class="flex flex-col gap-3" data-testid="storage-connection-form">
    <p v-if="missing" class="px-1 text-sm text-muted-foreground" role="status">
      {{ t('settings.storage.notFound') }}
    </p>

    <form v-else class="flex flex-col gap-3" @submit.prevent="saveAsync()">
      <SettingsGroup :label="t('settings.storage.groups.connection')">
        <SettingsRow
          :title="t('settings.storage.fields.protocol')"
          label-for="storage-protocol"
        >
          <SettingsSelect
            id="storage-protocol"
            :model-value="draft.protocol"
            :options="protocolOptions"
            :disabled="!isNew"
            data-testid="storage-protocol"
            @update:model-value="chooseProtocol"
          />
        </SettingsRow>
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
            v-if="presetHint"
            class="text-sm text-muted-foreground"
            data-testid="storage-preset-hint"
          >
            {{ presetHint }}
          </p>
          <p
            v-if="placeholdersLeft"
            class="text-sm text-destructive"
            data-testid="storage-endpoint-placeholders"
          >
            {{ t('settings.storage.placeholdersHint') }}
          </p>
          <p
            v-if="insecure"
            class="text-sm text-destructive"
            data-testid="storage-endpoint-insecure"
          >
            {{ t('settings.storage.insecureHint') }}
          </p>
        </li>
        <li v-if="!tree" class="px-4 py-3">
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
          v-if="!tree"
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
        <CredentialsMissingNote
          v-if="existing?.credentials === 'missing'"
          @choose="chooseSource"
        />
        <SettingsOptionRow
          v-if="!isNew"
          type="checkbox"
          :checked="replaceCredentials"
          :title="t('settings.storage.newCredentials')"
          data-testid="storage-replace-credentials"
          @change="replaceCredentials = $event"
        />
        <CredentialsLinkedEntry
          v-if="existing?.credentialsEntry"
          :entry="existing.credentialsEntry"
        />
        <SettingsRow
          v-if="withCredentials"
          :title="t('settings.storage.credentialsSource.label')"
          label-for="storage-credentials-source"
        >
          <SettingsSelect
            id="storage-credentials-source"
            v-model="credentials.source"
            :options="sourceOptions"
            data-testid="storage-credentials-source"
          />
        </SettingsRow>
        <CredentialsFields
          v-if="withCredentials && credentials.source === 'new'"
          v-model="credentials"
          :protocol="draft.protocol"
        />
      </SettingsGroup>
      <CredentialsEntryPicker
        v-if="withCredentials && credentials.source === 'entry'"
        v-model="credentials.entryId"
        :protocol="draft.protocol"
      />

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
            :label="
              tree
                ? t('settings.storage.fields.folder')
                : t('settings.storage.fields.bucket')
            "
            :labels="fieldLabels.input.value"
            label-bg="var(--muted)"
            autocomplete="off"
            data-testid="storage-bucket"
          />
          <p class="text-sm text-muted-foreground">
            {{
              tree
                ? t('settings.storage.testFolderHint')
                : t('settings.storage.testBucketHint')
            }}
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
      <HostKeyConfirm
        v-if="shown"
        :shown="shown.key"
        :previous="shown.changed ? existing?.hostKey?.fingerprint : undefined"
        :hint="
          draft.providerKind === 'hetznerStorageBox'
            ? t('settings.storage.hostKey.hetznerHint')
            : null
        "
        :busy="busy"
        @confirm="confirmHostKeyAsync"
        @cancel="shown = null"
      />
      <div class="flex flex-wrap gap-2">
        <UiButton type="submit" :loading="busy" data-testid="storage-save">
          {{
            busy ? t('settings.storage.testing') : t('settings.storage.save')
          }}
        </UiButton>
        <UiButton
          v-if="folderMissing"
          type="button"
          variant="outline"
          :disabled="busy"
          data-testid="storage-create-folder"
          @click="saveAsync(true)"
        >
          {{ t('settings.storage.createFolderAndSave') }}
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
