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

    <template #header>
      <h2 class="text-xl font-semibold">
        {{ t('title') }}
      </h2>
    </template>

    <template #body>
      <div
        v-if="path"
        class="text-sm text-gray-500 dark:text-gray-400 mb-4"
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
          v-model:errors="errors.password"
          :label="t('password.placeholder')"
          :schema="vaultSchema.password"
          :check="check"
          leading-icon="i-heroicons-key"
          size="xl"
          autofocus
          class="w-full"
          @keyup.enter="onOpenDatabase"
        />
      </UForm>
    </template>

    <template #footer>
      <div class="flex gap-3">
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
        base: 'px-4 py-3',
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
            v-model:errors="errors.password"
            :label="t('password.placeholder')"
            :schema="vaultSchema.password"
            :check="check"
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

const vault = reactive({
  name: '',
  password: '',
  path: '',
  type: 'password' as 'password' | 'text',
})

const errors = reactive({
  password: [] as string[],
})

const check = ref(false)

const initVault = () => {
  vault.name = ''
  vault.password = ''
  vault.path = ''
  vault.type = 'password'
}

const clearErrors = () => {
  errors.password = []
}

const onAbort = () => {
  initVault()
  clearErrors()
  check.value = false
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

    // Trigger validation
    check.value = true

    // Wait for validation to complete
    await nextTick()

    // If there are validation errors, don't proceed
    if (errors.password.length > 0) {
      return
    }

    const path = props.path
    const pathCheck = vaultSchema.path.safeParse(path)

    if (pathCheck.error) return

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
