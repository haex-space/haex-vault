<script setup lang="ts">
/**
 * Changing what an approved browser extension may do (spec 049, US5, FR-038): the same fields as
 * the approval question, never wider than the extension declared. Every complete choice is saved at
 * once through the guardrail action `settings.browserBridge.client.update`, without a save button,
 * and holds from the extension's next request on. Revoking asks first, in the settings view.
 */
import type { BridgeClientView } from '~/types/bindings/BridgeClientView'
import {
  draftOf,
  grantComplete,
  grantOf,
  type GrantDraft,
} from '~/lib/browserBridgeGrant'

const props = defineProps<{
  client: BridgeClientView | null
}>()

const emit = defineEmits<{
  close: []
  revoke: []
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const updateClient = useActionOrThrow('settings.browserBridge.client.update')

const draft = ref<GrantDraft>(draftOf('read', null, null))
/** The grant last saved, to skip saving what did not change. */
let saved = ''
const failure = ref<string | null>(null)

const fingerprint = computed(
  () => props.client?.clientId.match(/.{1,4}/g)?.join(' ') ?? '',
)

watch(
  () => props.client?.clientId,
  () => {
    const client = props.client
    failure.value = null
    if (!client?.access) return
    draft.value = draftOf(client.access, client.scopeTags, client.defaultTag)
    saved = JSON.stringify(grantOf(draft.value))
  },
  { immediate: true },
)

watch(draft, async (next) => {
  const client = props.client
  if (!client || !grantComplete(next)) return
  const grant = grantOf(next)
  const key = JSON.stringify(grant)
  if (key === saved) return
  saved = key
  try {
    await updateClient({
      clientId: client.clientId,
      access: grant.access,
      ...(grant.scopeTags ? { scopeTags: grant.scopeTags } : {}),
      ...(grant.defaultTag ? { defaultTag: grant.defaultTag } : {}),
    })
    failure.value = null
  } catch (error: unknown) {
    saved = ''
    failure.value = errString(error)
  }
})

function onUpdateOpen(open: boolean) {
  if (!open) emit('close')
}
</script>

<template>
  <UiDrawerModal
    v-if="client"
    :open="true"
    :title="t('browserBridge.clients.editTitle')"
    @update:open="onUpdateOpen"
  >
    <template #content>
      <div class="space-y-4" data-testid="bridge-client-editor">
        <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
          <dt class="text-muted-foreground">
            {{ t('browserBridge.client.name') }}
          </dt>
          <dd class="min-w-0 break-words">
            {{ client.clientName || t('browserBridge.client.noName') }}
            <span class="text-xs text-muted-foreground">
              ({{ t('browserBridge.client.claimed') }})
            </span>
          </dd>
          <dt class="text-muted-foreground">
            {{ t('browserBridge.client.fingerprint') }}
          </dt>
          <dd class="min-w-0 font-mono text-xs break-all">
            {{ fingerprint }}
          </dd>
          <dt class="text-muted-foreground">
            {{ t('browserBridge.client.origin') }}
          </dt>
          <dd class="min-w-0 font-mono text-xs break-all">
            {{ client.origin }}
          </dd>
        </dl>

        <BrowserBridgeGrantFields
          v-if="client.declared"
          v-model="draft"
          :declared="client.declared"
          :declares-write="client.declaresWrite"
          test-prefix="bridge-client"
        />
        <p v-else class="text-sm text-muted-foreground">
          {{ t('browserBridge.clients.noDeclaration') }}
        </p>
        <p class="text-xs text-muted-foreground">
          {{
            client.remembered
              ? t('browserBridge.approval.rememberedHint')
              : t('browserBridge.approval.sessionHint')
          }}
          {{ t('browserBridge.clients.savedHint') }}
        </p>
        <p v-if="failure" class="text-sm text-destructive" role="alert">
          {{ failure }}
        </p>
      </div>
    </template>
    <template #footer>
      <div class="flex justify-end">
        <UiButton
          type="button"
          size="sm"
          variant="destructive"
          data-testid="bridge-client-revoke"
          @click="emit('revoke')"
        >
          {{ t('browserBridge.clients.revoke') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
