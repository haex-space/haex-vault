<script setup lang="ts">
/**
 * "Allgemein → Grundeinstellung → Vaultpasswort ändern" (spec 042 US3, FR-011, FR-012): changes
 * the passphrase this device opens the vault with, after the current one is confirmed. Other
 * devices keep theirs. A location of its own, so an agent can open it with `wm.app.open` but
 * never change the passphrase itself (FR-014).
 */
import { toast } from 'vue-sonner'

const { t } = useI18n()
const { errString } = useErrorString()
const fieldLabels = useFieldLabels()
const { changePassphraseAsync } = useInstance()

const current = ref('')
const next = ref('')
const repeat = ref('')
const busy = ref(false)

/** Why the change cannot be submitted yet, `null` when it can. */
const problem = computed(() => {
  if (!current.value || !next.value) return null
  if (next.value.length < 8) return t('settings.password.tooShort')
  if (next.value === current.value) return t('settings.password.same')
  if (repeat.value && repeat.value !== next.value)
    return t('settings.password.mismatch')
  return null
})
const canSubmit = computed(
  () =>
    !busy.value &&
    current.value.length > 0 &&
    next.value.length >= 8 &&
    next.value !== current.value &&
    repeat.value === next.value,
)

async function submitAsync() {
  if (!canSubmit.value) return
  busy.value = true
  try {
    await changePassphraseAsync({ current: current.value, new: next.value })
    current.value = ''
    next.value = ''
    repeat.value = ''
    // The emptied form shows nothing of the change, so its outcome is a toast.
    toast.success(t('settings.password.changed'))
  } catch (e) {
    const kind =
      e && typeof e === 'object' && 'kind' in e
        ? (e as { kind: unknown }).kind
        : undefined
    toast.error(
      kind === 'WrongPassphrase'
        ? t('settings.password.wrongCurrent')
        : kind === 'WeakPassphrase'
          ? t('settings.password.weak')
          : errString(e),
    )
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <form class="flex flex-col gap-4" @submit.prevent="submitAsync">
    <p class="px-1 text-sm text-muted-foreground">
      {{ t('settings.password.deviceOnly') }}
    </p>
    <SettingsGroup>
      <div class="flex flex-col gap-4 p-4">
        <UiInputPassword
          id="password-change-current"
          v-model="current"
          :label="t('settings.password.current')"
          :labels="fieldLabels.password.value"
          autocomplete="current-password"
          data-testid="password-change-current"
        />
        <UiInputPassword
          id="password-change-new"
          v-model="next"
          :label="t('settings.password.new')"
          :labels="fieldLabels.password.value"
          autocomplete="new-password"
          data-testid="password-change-new"
        />
        <UiInputPassword
          id="password-change-repeat"
          v-model="repeat"
          :label="t('settings.password.repeat')"
          :labels="fieldLabels.password.value"
          autocomplete="new-password"
          data-testid="password-change-repeat"
        />
      </div>
    </SettingsGroup>

    <p v-if="problem" class="px-1 text-xs text-muted-foreground">
      {{ problem }}
    </p>
    <div class="flex justify-end">
      <UiButton
        type="submit"
        :disabled="!canSubmit"
        :loading="busy"
        data-testid="password-change-submit"
      >
        {{ t('settings.password.submit') }}
      </UiButton>
    </div>
  </form>
</template>
