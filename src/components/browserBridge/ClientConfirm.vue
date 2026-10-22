<script setup lang="ts">
/**
 * The confirmation before a browser extension is revoked or unblocked (spec 049, US5, FR-011): a
 * revoked one is disconnected at once, and both are asked again on their next connect.
 */
import type { BridgeClientView } from '~/types/bindings/BridgeClientView'

const props = defineProps<{
  kind: 'revoke' | 'unblock'
  client: BridgeClientView
}>()

const emit = defineEmits<{
  close: []
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const revoke = useActionOrThrow('settings.browserBridge.client.revoke')
const unblock = useActionOrThrow('settings.browserBridge.client.unblock')
const busy = ref(false)
const failure = ref<string | null>(null)

const name = computed(
  () => props.client.clientName || t('browserBridge.client.noName'),
)

async function confirmAsync() {
  busy.value = true
  failure.value = null
  try {
    const run = props.kind === 'revoke' ? revoke : unblock
    await run({ clientId: props.client.clientId })
    emit('close')
  } catch (error: unknown) {
    failure.value = errString(error)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <ShadcnAlertDialog
    :open="true"
    @update:open="(open: boolean) => !open && emit('close')"
  >
    <ShadcnAlertDialogContent>
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>
          {{ t(`browserBridge.clients.${kind}Title`) }}
        </ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription>
          {{ t(`browserBridge.clients.${kind}Body`, { name }) }}
        </ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <p v-if="failure" class="text-sm text-destructive" role="alert">
        {{ failure }}
      </p>
      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel>
          {{ t('browserBridge.clients.cancel') }}
        </ShadcnAlertDialogCancel>
        <UiButton
          :variant="kind === 'revoke' ? 'destructive' : 'default'"
          :loading="busy"
          data-testid="bridge-client-confirm"
          @click="confirmAsync"
        >
          {{ t(`browserBridge.clients.${kind}`) }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
