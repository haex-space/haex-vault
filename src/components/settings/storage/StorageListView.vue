<script setup lang="ts">
/**
 * The category "Cloud-Speicher" (spec 038, US1, T028): every connection as its own group, its row leads
 * to the connection form, below it its storages with bucket, last test on this device and the
 * extensions that may use it, and a row to add a storage. A connection whose credentials are not
 * here says so: "wird synchronisiert" while they are on their way from another device, "neue
 * Zugangsdaten nötig" after the user deleted them (data-model.md). A local endpoint (research R8)
 * and an unencrypted one are marked. Follows changes from every
 * writer and device (`useStorageOverview`).
 */
import type { ConnectionView } from '@bindings/ConnectionView'
import type { StorageView } from '@bindings/StorageView'

const { t } = useI18n()
const { overview, failure } = useStorageOverview()

const storagesOf = (connectionId: string): StorageView[] =>
  overview.value?.storages.filter((s) => s.connectionId === connectionId) ?? []

/** Whether a storage of the connection last met another host key than the confirmed one (SFTP,
 * spec 050 research R5): the storage's test shows both and takes the new one. */
const hostKeyChanged = (connectionId: string): boolean =>
  storagesOf(connectionId).some(
    (storage) => storage.lastTest?.outcome === 'hostKeyChanged',
  )

function connectionDescription(connection: ConnectionView): string {
  const where = connection.endpoint || t('settings.storage.kinds.aws')
  const protocol = t(`settings.storage.protocols.${connection.protocol}`)
  return connection.protocol === 's3'
    ? `${protocol} · ${where} · ${connection.region}`
    : `${protocol} · ${where}`
}

function storageDescription(storage: StorageView): string {
  const parts = [storage.bucket || '/']
  parts.push(
    storage.lastTest
      ? t('settings.storage.lastTest', {
          outcome: t(`settings.storage.outcome.${storage.lastTest.outcome}`),
          at: new Date(storage.lastTest.at).toLocaleString(),
        })
      : t('settings.storage.untested'),
  )
  if (storage.extensions.length > 0)
    parts.push(
      t('settings.storage.usedBy', {
        extensions: storage.extensions.join(', '),
      }),
    )
  return parts.join(' · ')
}
</script>

<template>
  <section class="flex flex-col gap-3" data-testid="storage-list">
    <p class="px-1 text-sm text-muted-foreground">
      {{ t('settings.storage.intro') }}
    </p>

    <SettingsGroup>
      <SettingsRow
        icon="lucide:plus"
        :title="t('settings.storage.addConnection')"
        :description="t('settings.storage.addConnectionHint')"
        to="/storage/connections/new"
        data-testid="storage-add-connection"
      />
    </SettingsGroup>

    <p v-if="failure" class="px-1 text-sm text-destructive" role="alert">
      {{ failure }}
    </p>
    <p
      v-else-if="overview && overview.connections.length === 0"
      class="px-1 text-sm text-muted-foreground"
    >
      {{ t('settings.storage.none') }}
    </p>

    <SettingsGroup
      v-for="connection in overview?.connections ?? []"
      :key="connection.id"
      :label="connection.providerName"
      :data-connection-id="connection.id"
      data-testid="storage-connection"
    >
      <SettingsRow
        icon="lucide:server"
        :title="t(`settings.storage.kinds.${connection.providerKind}`)"
        :description="connectionDescription(connection)"
        :to="`/storage/connections/${connection.id}`"
        data-testid="storage-connection-row"
      >
        <span
          v-if="connection.credentials !== 'present'"
          class="text-xs"
          :class="
            connection.credentials === 'missing'
              ? 'text-destructive'
              : 'text-muted-foreground'
          "
          :data-testid="`storage-credentials-${connection.credentials}`"
        >
          {{ t(`settings.storage.credentialsState.${connection.credentials}`) }}
        </span>
        <span
          v-if="connection.endpointScope === 'local'"
          class="rounded-md bg-muted px-2 py-0.5 text-xs text-muted-foreground"
          :title="t('settings.storage.localHint')"
          data-testid="storage-local"
        >
          {{ t('settings.storage.local') }}
        </span>
        <span
          v-if="hostKeyChanged(connection.id)"
          class="rounded-md bg-destructive/15 px-2 py-0.5 text-xs text-destructive"
          :title="t('settings.storage.hostKey.changedHint')"
          data-testid="storage-host-key-changed"
        >
          {{ t('settings.storage.outcome.hostKeyChanged') }}
        </span>
        <span
          v-if="connection.insecure"
          class="rounded-md bg-destructive/15 px-2 py-0.5 text-xs text-destructive"
          :title="t('settings.storage.insecureHint')"
          data-testid="storage-insecure"
        >
          {{ t('settings.storage.insecure') }}
        </span>
      </SettingsRow>
      <SettingsRow
        v-for="storage in storagesOf(connection.id)"
        :key="storage.id"
        icon="lucide:archive"
        :title="storage.name"
        :description="storageDescription(storage)"
        :to="`/storage/connections/${connection.id}/storages/${storage.id}`"
        :data-storage-id="storage.id"
        data-testid="storage-row"
      />
      <SettingsRow
        icon="lucide:plus"
        :title="t('settings.storage.addStorage')"
        :to="`/storage/connections/${connection.id}/storages/new`"
        data-testid="storage-add-storage"
      />
    </SettingsGroup>
  </section>
</template>
