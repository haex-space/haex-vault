<template>
  <div>
    <div
      v-if="passkeys.length === 0"
      class="py-6 flex flex-col items-center gap-2 text-muted"
    >
      <UIcon
        name="i-lucide-key-round"
        class="size-8 opacity-40"
      />
      <p class="text-sm">
        {{ t('empty') }}
      </p>
    </div>

    <div
      v-else
      class="divide-y divide-default"
    >
      <HaexSystemPasswordsEditorPasskeysEntry
        v-for="pk in passkeys"
        :key="pk.id"
        :passkey="pk"
        :read-only="readOnly"
        @delete="onDelete"
        @update-nickname="onUpdateNickname"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { eq } from 'drizzle-orm'
import { haexPasswordsPasskeys } from '~/database/schemas'
import type { SelectHaexPasswordsPasskeys } from '~/database/schemas'
import { requireDb } from '~/stores/vault'

const props = defineProps<{
  itemId?: string
  readOnly?: boolean
}>()

const { t } = useI18n()

const savedPasskeys = ref<SelectHaexPasswordsPasskeys[]>([])
const passkeysToDelete = ref<Set<string>>(new Set())

const passkeys = computed(() =>
  savedPasskeys.value.filter((pk) => !passkeysToDelete.value.has(pk.id)),
)

const loadAsync = async () => {
  if (!props.itemId) {
    savedPasskeys.value = []
    return
  }
  const db = requireDb()
  savedPasskeys.value = await db
    .select()
    .from(haexPasswordsPasskeys)
    .where(eq(haexPasswordsPasskeys.itemId, props.itemId))
}

watch(() => props.itemId, loadAsync, { immediate: true })

const onDelete = (passkeyId: string) => {
  passkeysToDelete.value.add(passkeyId)
}

const onUpdateNickname = async (passkeyId: string, nickname: string) => {
  const db = requireDb()
  await db
    .update(haexPasswordsPasskeys)
    .set({ nickname })
    .where(eq(haexPasswordsPasskeys.id, passkeyId))
  const pk = savedPasskeys.value.find((p) => p.id === passkeyId)
  if (pk) pk.nickname = nickname
}

const persistDeletionsAsync = async () => {
  if (passkeysToDelete.value.size === 0) return
  const db = requireDb()
  for (const id of passkeysToDelete.value) {
    await db
      .delete(haexPasswordsPasskeys)
      .where(eq(haexPasswordsPasskeys.id, id))
  }
  passkeysToDelete.value.clear()
  await loadAsync()
}

defineExpose({ persistDeletionsAsync })
</script>

<i18n lang="yaml">
de:
  empty: Keine Passkeys vorhanden. Passkeys werden automatisch erstellt, wenn du dich über die Browser-Erweiterung auf einer Website mit Passkey-Unterstützung registrierst.

en:
  empty: No passkeys available. Passkeys are created automatically when you register on a website with passkey support via the browser extension.
</i18n>
