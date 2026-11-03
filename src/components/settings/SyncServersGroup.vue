<script setup lang="ts">
/**
 * The servers devices find each other through (spec 024, FR-008): Nostr-Relays for the presence
 * meetings and iroh-Relays for the connections. Each change is saved at once, without a save button
 * (spec 023 FR-021). The built-in public servers are always listed and can be switched off, not
 * removed; added ones can be switched off or removed. The iroh-Relays apply at once, the
 * Nostr-Relays when the vault is opened next.
 */
import { toast } from 'vue-sonner'
import type { SyncServers } from '@bindings/SyncServers'
import {
  serverEntries,
  withEnabled,
  withoutServer,
} from '~/lib/sync/serverEntries'

const { t } = useI18n()
const { errString } = useErrorString()
const { syncServersGetAsync, syncServersDefaultsAsync } = useSync()
const setServers = useActionOrThrow('settings.sync.servers.set')

const servers = ref<SyncServers>({
  nostrRelays: [],
  irohRelays: [],
  disabled: [],
})
const defaults = ref<SyncServers>({
  nostrRelays: [],
  irohRelays: [],
  disabled: [],
})
const loaded = ref(false)
const busy = ref(false)

async function loadAsync() {
  try {
    ;[servers.value, defaults.value] = await Promise.all([
      syncServersGetAsync(),
      syncServersDefaultsAsync(),
    ])
    loaded.value = true
  } catch (e) {
    toast.error(errString(e))
  }
}

onMounted(loadAsync)
// The servers are vault preferences: a change made on another device shows here at once.
onVaultTablesChanged(['preferences'], () => {
  if (!busy.value) return loadAsync()
})

type Kind = 'nostrRelays' | 'irohRelays'

async function save(next: SyncServers): Promise<boolean> {
  busy.value = true
  try {
    await setServers({ ...next })
    servers.value = next
    return true
  } catch (e) {
    const reason = (e as { reason?: string })?.reason
    toast.error(reason ?? errString(e))
    return false
  } finally {
    busy.value = false
  }
}

function addAsync(kind: Kind, url: string): Promise<boolean> {
  return save({
    ...servers.value,
    [kind]: [...servers.value[kind], url],
  })
}

async function onToggle(url: string, enabled: boolean) {
  if (busy.value) return
  await save({
    ...servers.value,
    disabled: withEnabled(servers.value.disabled, url, enabled),
  })
}

async function onRemove(kind: Kind, url: string) {
  if (busy.value) return
  const rest = withoutServer(servers.value[kind], servers.value.disabled, url)
  await save({ ...servers.value, [kind]: rest.added, disabled: rest.disabled })
}

const groups = computed(() => [
  {
    kind: 'nostrRelays' as const,
    label: t('settings.federation.servers.nostr'),
    description: t('settings.federation.servers.nostrDescription'),
    placeholder: 'wss://',
    noneNote: t('settings.federation.servers.nostrNone'),
  },
  {
    kind: 'irohRelays' as const,
    label: t('settings.federation.servers.iroh'),
    description: t('settings.federation.servers.irohDescription'),
    placeholder: 'https://',
    noneNote: t('settings.federation.servers.irohNone'),
  },
])
</script>

<template>
  <section v-if="loaded" class="flex flex-col gap-3">
    <SettingsServerList
      v-for="group in groups"
      :key="group.kind"
      :label="group.label"
      :description="group.description"
      :entries="
        serverEntries(
          defaults[group.kind],
          servers[group.kind],
          servers.disabled,
        )
      "
      :placeholder="group.placeholder"
      :none-note="group.noneNote"
      :test-id="`settings-servers-${group.kind}`"
      :busy="busy"
      :add-async="(url: string) => addAsync(group.kind, url)"
      @toggle="onToggle"
      @remove="onRemove(group.kind, $event)"
    />
  </section>
</template>
