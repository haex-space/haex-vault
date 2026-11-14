<script setup lang="ts">
/**
 * "Pin to home screen" for one pass (spec 092, FR-038): asks for the label of the shortcut (a
 * neutral default, never the title: the shortcut sits on the home screen where anyone can read it),
 * then asks the launcher to pin it. The launcher confirms with the person itself, so there is no
 * notice here; a failure is a toast. Offered only where the launcher can pin and the pass is not in
 * the trash.
 */
import { toast } from 'vue-sonner'

const props = defineProps<{ itemId: string }>()

const { t } = useI18n()
const { errString } = useErrorString()
const fieldLabels = useFieldLabels()
const { supported, probeAsync, pinAsync } = usePassShortcut()

const open = ref(false)
const label = ref('')
const pinning = ref(false)

onMounted(probeAsync)

function ask() {
  label.value = t('wallet.shortcut.defaultLabel')
  open.value = true
}

async function pinOneAsync() {
  if (pinning.value || label.value.trim() === '') return
  pinning.value = true
  try {
    await pinAsync(props.itemId, label.value)
    open.value = false
  } catch (cause) {
    toast.error(errString(cause))
  } finally {
    pinning.value = false
  }
}
</script>

<template>
  <SettingsGroup v-if="supported">
    <SettingsRow
      :title="t('wallet.shortcut.pin')"
      :description="t('wallet.shortcut.pinHint')"
      icon="lucide:pin"
      navigates
      data-testid="wallet-pin-shortcut"
      @click="ask"
    />
  </SettingsGroup>
  <UiDrawerModal v-model:open="open" :title="t('wallet.shortcut.title')">
    <template #content>
      <div class="flex flex-col gap-4" data-testid="wallet-pin-dialog">
        <p class="text-sm text-muted-foreground">
          {{ t('wallet.shortcut.intro') }}
        </p>
        <UiInput
          id="wallet-shortcut-label"
          v-model="label"
          :label="t('wallet.shortcut.label')"
          :labels="fieldLabels.input.value"
          label-bg="var(--muted)"
          maxlength="40"
          data-testid="wallet-shortcut-label"
          @keydown.enter.prevent="pinOneAsync"
        />
        <div class="flex flex-wrap justify-end gap-2">
          <UiButton
            type="button"
            variant="outline"
            :disabled="pinning"
            @click="open = false"
          >
            {{ t('passwords.cancel') }}
          </UiButton>
          <UiButton
            type="button"
            :loading="pinning"
            :disabled="label.trim() === ''"
            data-testid="wallet-shortcut-pin"
            @click="pinOneAsync"
          >
            {{ t('wallet.shortcut.pinNow') }}
          </UiButton>
        </div>
      </div>
    </template>
  </UiDrawerModal>
</template>
