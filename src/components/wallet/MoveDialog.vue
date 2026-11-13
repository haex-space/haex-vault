<script setup lang="ts">
/**
 * Moves one pass into a folder of Wallet, or out of every folder (spec 092, Phase W). It does what
 * dragging onto the sidebar does, for a touch screen and the keyboard: "No folder" is the top level
 * ("All"), then Wallet's folders in tree order with their depth. The folder the pass is in now is
 * marked. Only ids go to the backend, which refuses a place the rules do not allow.
 */
import { displayTitle } from '~/lib/passwords/format'
import { buildTree, flattenFolders } from '~/lib/passwords/tree'

const props = defineProps<{ itemId: string }>()
const open = defineModel<boolean>('open', { required: true })

const { t } = useI18n()
const store = usePasswordsStore()
const actions = usePasswordsActions()

const folders = computed(() =>
  flattenFolders(buildTree(store.walletFolders, []).roots),
)
const currentId = computed(
  () => store.headersById.get(props.itemId)?.groupId ?? null,
)

async function moveAsync(groupId: string | null) {
  open.value = false
  if (groupId === currentId.value) return
  await actions.moveIdsAsync([props.itemId], groupId)
}
</script>

<template>
  <UiDrawerModal v-model:open="open" :title="t('wallet.move.title')">
    <template #content>
      <div class="flex flex-col gap-3" data-testid="wallet-move-dialog">
        <SettingsGroup>
          <SettingsRow
            :title="t('wallet.move.none')"
            :description="
              currentId === null ? t('wallet.move.current') : undefined
            "
            icon="lucide:wallet"
            navigates
            data-testid="wallet-move-none"
            @click="moveAsync(null)"
          />
          <SettingsRow
            v-for="folder in folders"
            :key="folder.id"
            :title="displayTitle(folder.name) ?? t('passwords.folders.unknown')"
            :description="
              currentId === folder.id ? t('wallet.move.current') : undefined
            "
            icon="lucide:folder"
            navigates
            :style="{ paddingLeft: `${folder.depth * 16}px` }"
            :data-testid="`wallet-move-${folder.id}`"
            @click="moveAsync(folder.id)"
          />
        </SettingsGroup>
        <p
          v-if="folders.length === 0"
          class="text-center text-sm text-muted-foreground"
        >
          {{ t('wallet.move.empty') }}
        </p>
      </div>
    </template>
  </UiDrawerModal>
</template>
