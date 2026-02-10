<template>
  <div class="h-full relative">
    <!-- Version display in bottom right corner -->
    <span class="absolute bottom-2 right-3 text-xs text-muted opacity-50">
      v{{ appVersion }}
    </span>
    <NuxtLayout>
      <div
        class="flex flex-col justify-center items-center gap-5 mx-auto h-full overflow-auto"
      >
        <UiLogoHaexspace class="size-16 shrink-0" />
        <span
          class="flex flex-wrap font-bold text-pretty text-xl gap-2 justify-center"
        >
          <p class="whitespace-nowrap">
            {{ t('welcome') }}
          </p>
          <UiTextGradient>Haex Space</UiTextGradient>
        </span>

        <div class="flex flex-col gap-3 w-56 items-stretch">
          <HaexVaultCreate v-model:open="isCreateDrawerOpen" />

          <HaexVaultImport v-model:open="isImportDrawerOpen" />
        </div>

        <!-- Hidden component for opening vaults from the list -->
        <HaexVaultOpen
          v-model:open="isOpenDrawerOpen"
          :path="selectedVault?.path"
          :name="selectedVault?.name"
        />

        <div
          v-show="lastVaults.length"
          class="w-56"
        >
          <div class="font-thin text-sm pb-1 w-full">
            {{ t('lastUsed') }}
          </div>

          <div
            class="relative flex w-full flex-col gap-1.5"
          >
            <div
              v-for="vault in lastVaults"
              :key="vault.name"
              class="flex items-center justify-between group overflow-x-hidden rounded-lg bg-black/5 dark:bg-white/5 ring-1 ring-black/10 dark:ring-white/10 hover:bg-black/10 dark:hover:bg-white/10 hover:ring-black/20 dark:hover:ring-white/20 transition-all"
            >
              <UiButtonContext
                variant="ghost"
                color="neutral"
                size="xl"
                class="flex items-center no-underline justify-between text-nowrap text-sm shrink w-full"
                :context-menu-items="[
                  {
                    icon: 'mdi:trash-can-outline',
                    label: t('remove.button'),
                    onSelect: () => prepareRemoveVault(vault.name),
                    color: 'error',
                  },
                ]"
                :ui="{
                  base: 'px-4 py-3',
                }"
                @click="
                  () => {
                    isOpenDrawerOpen = true
                    selectedVault = vault
                  }
                "
              >
                <span class="block">
                  {{ vault.name }}
                </span>
              </UiButtonContext>
              <UButton
                color="error"
                square
                class="absolute right-2 hidden group-hover:flex min-w-6"
              >
                <Icon
                  name="mdi:trash-can-outline"
                  @click="prepareRemoveVault(vault.name)"
                />
              </UButton>
            </div>
          </div>
        </div>

        <div class="flex flex-col items-center gap-2">
          <h4>{{ t('sponsors') }}</h4>
          <div>
            <UButton
              variant="link"
              @click="openUrl('https://itemis.com')"
            >
              <UiLogoItemis class="text-[#00457C]" />
            </UButton>
          </div>
        </div>
      </div>

      <UiDialogConfirm
        v-model:open="showRemoveDialog"
        :title="t('remove.title')"
        :description="t('remove.description', { vaultName: vaultToBeRemoved })"
        @confirm="onConfirmRemoveAsync"
      />
    </NuxtLayout>
  </div>
</template>

<script setup lang="ts">
import { openUrl } from '@tauri-apps/plugin-opener'
import { getVersion } from '@tauri-apps/api/app'

import type { VaultInfo } from '@bindings/VaultInfo'

definePageMeta({
  name: 'vaultOpen',
})

const { t } = useI18n()

const appVersion = ref('')

const isCreateDrawerOpen = ref(false)
const isImportDrawerOpen = ref(false)
const isOpenDrawerOpen = ref(false)
const selectedVault = ref<VaultInfo>()

// Ensure only one drawer is open at a time
watch(isCreateDrawerOpen, (isOpen) => {
  if (isOpen) {
    isImportDrawerOpen.value = false
    isOpenDrawerOpen.value = false
  }
})

watch(isImportDrawerOpen, (isOpen) => {
  if (isOpen) {
    isCreateDrawerOpen.value = false
    isOpenDrawerOpen.value = false
  }
})

watch(isOpenDrawerOpen, (isOpen) => {
  if (isOpen) {
    isCreateDrawerOpen.value = false
    isImportDrawerOpen.value = false
  }
})

const showRemoveDialog = ref(false)

const { lastVaults } = storeToRefs(useLastVaultStore())

const { syncLastVaultsAsync, moveVaultToTrashAsync } = useLastVaultStore()
const { syncDeviceIdAsync } = useDeviceStore()

const vaultToBeRemoved = ref('')
const prepareRemoveVault = (vaultName: string) => {
  vaultToBeRemoved.value = vaultName
  showRemoveDialog.value = true
}

const toast = useToast()
const onConfirmRemoveAsync = async () => {
  try {
    await moveVaultToTrashAsync(vaultToBeRemoved.value)
    showRemoveDialog.value = false
    await syncLastVaultsAsync()
  } catch (error) {
    toast.add({
      color: 'error',
      description: JSON.stringify(error),
    })
  }
}

onMounted(async () => {
  try {
    appVersion.value = await getVersion()
    await syncLastVaultsAsync()
    await syncDeviceIdAsync()
  } catch (error) {
    console.error('ERROR: ', error)
  }
})
</script>

<i18n lang="yaml">
de:
  welcome: 'Viel Spass im'
  lastUsed: 'Zuletzt verwendete Vaults'
  sponsors: Supported by
  remove:
    button: Löschen
    title: Vault löschen
    description: Möchtest du die Vault {vaultName} wirklich löschen?

en:
  welcome: 'Have fun at'
  lastUsed: 'Last used Vaults'
  sponsors: 'Supported by'
  remove:
    button: Delete
    title: Delete Vault
    description: Are you sure you really want to delete {vaultName}?
</i18n>
