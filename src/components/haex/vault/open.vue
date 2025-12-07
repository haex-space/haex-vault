<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
    :description="path || t('description')"
  >
    <!-- No trigger - this component is opened programmatically -->
    <template #trigger>
      <span class="hidden" />
    </template>

    <!-- Content -->
    <template #content>
      <div class="space-y-4">
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
            v-model:errors="errors.password"
            :label="t('password.placeholder')"
            :schema="vaultSchema.password"
            :check="check"
            leading-icon="i-lucide-lock"
            size="xl"
            autofocus
            class="w-full"
            @keyup.enter="onOpenDatabase"
          />
        </UForm>

        <div class="flex gap-3 pt-4">
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
  </UiDrawerModal>
</template>

<script setup lang="ts">
import { revealItemInDir } from '@tauri-apps/plugin-opener'
import { vaultSchema } from './schema'

const open = defineModel<boolean>('open', { default: false })
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

    console.log('[VAULT OPEN] onOpenDatabase called')
    console.log('[VAULT OPEN] path:', props.path)

    const { openAsync } = useVaultStore()
    const localePath = useLocalePath()

    // Trigger validation
    check.value = true

    // Wait for validation to complete
    await nextTick()

    // If there are validation errors, don't proceed
    if (errors.password.length > 0) {
      console.log('[VAULT OPEN] Validation errors, aborting')
      return
    }

    const path = props.path
    const pathCheck = vaultSchema.path.safeParse(path)

    if (pathCheck.error) {
      console.log('[VAULT OPEN] Path validation failed:', pathCheck.error)
      return
    }

    console.log('[VAULT OPEN] Calling vaultStore.openAsync...')

    const vaultId = await openAsync({
      path,
      password: vault.password,
    })

    console.log('[VAULT OPEN] openAsync returned vaultId:', vaultId)

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
