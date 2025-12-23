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
            size="lg"
            autofocus
            class="w-full"
            @keyup.enter="onOpenDatabase"
          />
        </UForm>

        <!-- Biometry retry button -->
        <UButton
          v-if="hasBiometryData"
          color="primary"
          variant="outline"
          block
          size="lg"
          icon="i-lucide-fingerprint"
          @click="onBiometryUnlock"
        >
          {{ t('biometry.retry') }}
        </UButton>

        <div class="flex gap-3 pt-4">
          <UButton
            color="neutral"
            variant="outline"
            block
            size="lg"
            @click="open = false"
          >
            {{ t('cancel') }}
          </UButton>
          <UButton
            color="primary"
            block
            size="lg"
            @click="onOpenDatabase()"
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
import { isMobile } from '~/utils/platform'
import { useBiometry } from '~/composables/useBiometry'

const open = defineModel<boolean>('open', { default: false })
const props = defineProps<{
  path?: string
  name?: string
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

// Biometry state
const biometry = useBiometry()
const isBiometryAvailable = ref(false)
const hasBiometryData = ref(false)

// Check biometry availability when modal opens
watch(open, async (isOpen) => {
  if (isOpen && isMobile() && props.name) {
    try {
      const status = await biometry.checkStatus()
      isBiometryAvailable.value = status.isAvailable

      if (status.isAvailable) {
        hasBiometryData.value = await biometry.hasData({
          domain: 'haex-vault',
          name: props.name,
        })

        // Auto-trigger biometric unlock if data exists
        if (hasBiometryData.value) {
          await onBiometryUnlock()
        }
      }
    } catch {
      isBiometryAvailable.value = false
    }
  }
})

// Unlock with biometry
const onBiometryUnlock = async () => {
  if (!props.name) return

  const password = await biometry.getData({
    domain: 'haex-vault',
    name: props.name,
    reason: t('biometry.reason'),
  })

  if (password) {
    vault.password = password
    await onOpenDatabase({ fromBiometry: true })
  }
}

// Save password to biometry after successful unlock
const saveToBiometry = async (password: string) => {
  if (!isBiometryAvailable.value || !props.name) return

  try {
    await biometry.setData({
      domain: 'haex-vault',
      name: props.name,
      data: password,
    })
    hasBiometryData.value = true
  } catch (e) {
    console.warn('[Biometry] Save failed:', e)
  }
}

// Remove biometry data (when password is wrong)
const removeBiometryData = async () => {
  if (!props.name) return

  try {
    await biometry.removeData({
      domain: 'haex-vault',
      name: props.name,
    })
    hasBiometryData.value = false
  } catch (e) {
    console.warn('[Biometry] Remove failed:', e)
  }
}

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

const onOpenDatabase = async (options?: { fromBiometry?: boolean }) => {
  const fromBiometry = options?.fromBiometry ?? false

  try {
    if (!props.path) return

    const { openAsync } = useVaultStore()
    const localePath = useLocalePath()

    // Skip validation if coming from biometry (password is pre-filled)
    if (!fromBiometry) {
      // Trigger validation
      check.value = true

      // Wait for validation to complete
      await nextTick()

      // If there are validation errors, don't proceed
      if (errors.password.length > 0) {
        return
      }
    }

    const path = props.path
    const pathCheck = vaultSchema.path.safeParse(path)

    if (pathCheck.error) {
      return
    }

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

    // Save password to biometry on successful unlock (if not already from biometry)
    if (!fromBiometry && isBiometryAvailable.value) {
      await saveToBiometry(vault.password)
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
      // Wrong password - remove biometry data if it came from biometry
      if (fromBiometry) {
        await removeBiometryData()
      }

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
  biometry:
    reason: Entsperre deine Vault
    retry: Mit Fingerabdruck entsperren
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
  biometry:
    reason: Unlock your vault
    retry: Unlock with fingerprint
  error:
    open: Vault couldn't be opened
    password:
      title: Vault couldn't be opened
      description: Please check your password
</i18n>
