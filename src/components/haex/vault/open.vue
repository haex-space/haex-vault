<template>
  <UiDrawer
    v-if="isSmallScreen"
    v-model:open="open"
    :title="t('title')"
    :description="path || t('description')"
  >
    <UiButton
      :label="t('button.label')"
      :ui="{
        base: 'px-4 py-3',
      }"
      icon="mdi:folder-open-outline"
      size="xl"
      variant="outline"
      block
    />

    <template #content>
      <div class="p-6 flex flex-col">
        <div class="w-full mx-auto space-y-4">
          <h2 class="text-xl font-semibold">
            {{ t('title') }}
          </h2>

          <div
            v-if="path"
            class="text-sm text-gray-500 dark:text-gray-400"
          >
            <button
              class="text-primary hover:underline cursor-pointer break-all text-left"
              @click="onRevealInFolder"
            >
              {{ path }}
            </button>
          </div>

          <UForm
            :state="vault"
            class="w-full"
          >
            <UiInputPassword
              v-model="vault.password"
              :label="t('password.placeholder')"
              leading-icon="i-heroicons-key"
              size="xl"
              autofocus
              class="w-full"
              @keyup.enter="onOpenDatabase"
            />
          </UForm>
        </div>

        <div class="flex gap-3 mt-12">
          <UButton
            color="neutral"
            variant="outline"
            block
            size="xl"
            @click="open = false"
          >
            {{ t('cancel') }}
          </UButton>
          <UButton
            color="primary"
            block
            size="xl"
            @click="onOpenDatabase"
          >
            {{ t('open') }}
          </UButton>
        </div>
      </div>
    </template>
  </UiDrawer>

  <UModal
    v-else
    v-model:open="open"
    :title="t('title')"
    :description="path || t('description')"
  >
    <UiButton
      :label="t('button.label')"
      :ui="{
        base: 'px-3 py-2',
      }"
      icon="mdi:folder-open-outline"
      size="xl"
      variant="outline"
      block
    />

    <template #body>
      <div class="space-y-4">
        <UForm
          :state="vault"
          class="w-full"
        >
          <UiInputPassword
            v-model="vault.password"
            :label="t('password.placeholder')"
            leading-icon="i-heroicons-key"
            size="xl"
            autofocus
            class="w-full"
            @keyup.enter="onOpenDatabase"
          />
        </UForm>
      </div>
    </template>

    <template #footer>
      <div class="flex gap-3 w-full">
        <UButton
          color="neutral"
          variant="outline"
          block
          @click="open = false"
        >
          {{ t('cancel') }}
        </UButton>
        <UButton
          color="primary"
          block
          @click="onOpenDatabase"
        >
          {{ t('open') }}
        </UButton>
      </div>
    </template>
  </UModal>
</template>

<script setup lang="ts">
import { revealItemInDir } from '@tauri-apps/plugin-opener'
/* import { open as openVault } from '@tauri-apps/plugin-dialog' */
import { vaultSchema } from './schema'

const open = defineModel<boolean>('open', { default: false })
const { isSmallScreen } = storeToRefs(useUiStore())
const props = defineProps<{
  path?: string
}>()

const { t } = useI18n({
  useScope: 'local',
})

const vault = reactive<{
  name: string
  password: string
  path: string | null
  type: 'password' | 'text'
}>({
  name: '',
  password: '',
  path: '',
  type: 'password',
})

/* const onLoadDatabase = async () => {
  try {
    vault.path = await openVault({
      multiple: false,
      directory: false,
      filters: [
        {
          name: 'HaexVault',
          extensions: ['db'],
        },
      ],
    })

    console.log('onLoadDatabase', vault.path)
    if (!vault.path) {
      open.value = false
      return
    }

    open.value = true
  } catch (error) {
    open.value = false
    console.error('handleError', error, typeof error)
    add({ color: 'error', description: `${error}` })
  }
} */

const check = ref(false)

const initVault = () => {
  vault.name = ''
  vault.password = ''
  vault.path = ''
  vault.type = 'password'
}

const onAbort = () => {
  initVault()
  open.value = false
}

const { add } = useToast()

const onRevealInFolder = async () => {
  if (!props.path) return

  try {
    await revealItemInDir(props.path)
  } catch (error) {
    add({ color: 'error', description: `${error}` })
  }
}

const onOpenDatabase = async () => {
  try {
    if (!props.path) return

    const { openAsync } = useVaultStore()
    const localePath = useLocalePath()

    check.value = true
    const path = props.path
    const pathCheck = vaultSchema.path.safeParse(path)
    const passwordCheck = vaultSchema.password.safeParse(vault.password)

    if (pathCheck.error || passwordCheck.error) return

    const vaultId = await openAsync({
      path,
      password: vault.password,
    })

    if (!vaultId) {
      add({
        color: 'error',
        description: t('error.open'),
      })
      return
    }

    onAbort()

    await navigateTo(
      localePath({
        name: 'desktop',
        params: {
          vaultId,
        },
      }),
    )

    // Auto-login and start sync after vault is fully opened (non-blocking)
    const { autoLoginAndStartSyncAsync } = useVaultStore()
    autoLoginAndStartSyncAsync().catch((error) => {
      console.warn('[HaexSpace] Auto-login and sync start failed:', error)
    })
  } catch (error) {
    open.value = false
    const errorDetails =
      error && typeof error === 'object' && 'details' in error
        ? (error as { details?: { reason?: string } }).details
        : undefined

    if (errorDetails?.reason === 'file is not a database') {
      add({
        color: 'error',
        title: t('error.password.title'),
        description: t('error.password.description'),
      })
    } else {
      add({ color: 'error', description: JSON.stringify(error) })
    }
  }
}
</script>

<i18n lang="yaml">
de:
  button:
    label: Vault öffnen
  open: Entsperren
  cancel: Abbrechen
  title: HaexVault entsperren
  path:
    label: Pfad
  password:
    label: Passwort
    placeholder: Passwort eingeben
  description: Öffne eine vorhandene Vault
  error:
    open: Vault konnte nicht geöffnet werden
    password:
      title: Vault konnte nicht geöffnet werden
      description: Bitte überprüfe das Passwort

en:
  button:
    label: Open Vault
  open: Unlock
  cancel: Cancel
  title: Unlock HaexVault
  path:
    label: Path
  password:
    label: Password
    placeholder: Enter password
  description: Open your existing vault
  error:
    open: Vault couldn't be opened
    password:
      title: Vault couldn't be opened
      description: Please check your password
</i18n>
