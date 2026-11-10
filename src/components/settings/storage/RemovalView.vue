<script setup lang="ts">
/**
 * Removing a connection or a storage (spec 038, FR-007): before anything happens it lists the
 * storages that go and the extensions that lose access, says that a connection's credentials are
 * deleted from the password manager and that the data in the bucket stays at the provider. The
 * location names what goes: `/storage/remove/connection/<id>` or `/storage/remove/storage/<id>`.
 */
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'vue-sonner'
import type { RemovalPreview } from '@bindings/RemovalPreview'

const { t } = useI18n()
const router = useTabRouter()
const storageError = useStorageError()
const { overview, connection } = useStorageOverview()

const kind = computed(() =>
  router.route.params.kind === 'storage' ? 'storage' : 'connection',
)
const id = computed(() => router.route.params.id ?? '')
const name = computed(() =>
  kind.value === 'connection'
    ? connection(id.value)?.providerName
    : overview.value?.storages.find((s) => s.id === id.value)?.name,
)

const preview = ref<RemovalPreview | null>(null)
const busy = ref(false)

onMounted(async () => {
  try {
    preview.value = await invoke<RemovalPreview>('storage_removal_preview', {
      target:
        kind.value === 'connection'
          ? { connectionId: id.value }
          : { storageId: id.value },
    })
  } catch (error) {
    toast.error(storageError(error))
  }
})

async function removeAsync() {
  if (busy.value) return
  busy.value = true
  try {
    await invoke(
      kind.value === 'connection'
        ? 'storage_connection_remove'
        : 'storage_remove',
      { id: id.value },
    )
    router.push('/storage')
  } catch (error) {
    toast.error(storageError(error))
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <section class="flex flex-col gap-3" data-testid="storage-removal">
    <template v-if="preview">
      <SettingsGroup
        :label="
          kind === 'connection'
            ? t('settings.storage.removal.connectionTitle', {
                name: name ?? '',
              })
            : t('settings.storage.removal.storageTitle', { name: name ?? '' })
        "
      >
        <li class="flex flex-col gap-2 px-4 py-4 text-sm">
          <template v-if="preview.storages.length > 0">
            <p>{{ t('settings.storage.removal.storages') }}</p>
            <ul
              class="list-disc space-y-1 pl-5"
              data-testid="storage-removal-storages"
            >
              <li v-for="storage in preview.storages" :key="storage">
                {{ storage }}
              </li>
            </ul>
          </template>
          <template v-if="preview.extensions.length > 0">
            <p>{{ t('settings.storage.removal.extensions') }}</p>
            <ul
              class="list-disc space-y-1 pl-5"
              data-testid="storage-removal-extensions"
            >
              <li v-for="extension in preview.extensions" :key="extension">
                {{ extension }}
              </li>
            </ul>
          </template>
          <p v-if="kind === 'connection'">
            {{ t('settings.storage.removal.credentials') }}
          </p>
          <p>{{ t('settings.storage.removal.objects') }}</p>
        </li>
      </SettingsGroup>
    </template>
    <div v-if="preview" class="flex gap-2">
      <UiButton
        variant="destructive"
        :loading="busy"
        data-testid="storage-removal-confirm"
        @click="removeAsync"
      >
        {{ t('settings.storage.removal.confirm') }}
      </UiButton>
      <UiButton
        variant="outline"
        :disabled="busy"
        data-testid="storage-removal-cancel"
        @click="router.back()"
      >
        {{ t('settings.storage.cancel') }}
      </UiButton>
    </div>
  </section>
</template>
