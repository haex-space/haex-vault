<script setup lang="ts">
/**
 * May a browser extension connect, and with which grant? (spec 049, US1, FR-008 to FR-010,
 * contracts/tauri-commands.md). Shows what the client claims (its name, marked as its own claim),
 * its fingerprint and origin, and what it declared; the user picks the kind and the scope, never
 * wider than declared, and a default tag for new entries. Closing the dialog declines once, without
 * remembering. Rust checks the answer again; an answer it refuses counts as declining.
 */
import type { ApprovalQuestion } from '~/types/bindings/ApprovalQuestion'
import {
  draftOf,
  grantComplete,
  grantOf,
  type GrantDraft,
} from '~/lib/browserBridgeGrant'

const { t } = useI18n()
const { errString } = useErrorString()
const store = useBrowserBridgeStore()

const question = computed<ApprovalQuestion | null>(() =>
  store.shown?.question.kind === 'approval' ? store.shown.question : null,
)

const draft = ref<GrantDraft>(draftOf('read', null, null))
const remember = ref(false)
const failure = ref<string | null>(null)

const canAllow = computed(() => grantComplete(draft.value))

watch(
  () => store.shown?.promptId,
  () => {
    const asked = question.value
    failure.value = null
    remember.value = false
    if (!asked) return
    draft.value = draftOf(asked.declared.access, asked.declared.scopeTags, null)
  },
  { immediate: true },
)

/** The fingerprint in groups of four, as the settings show it. */
const fingerprint = computed(
  () => question.value?.client.clientId.match(/.{1,4}/g)?.join(' ') ?? '',
)

async function answerAsync(allow: boolean) {
  const shown = store.shown
  if (!shown) return
  const grant = grantOf(draft.value)
  try {
    await store.answerAsync(shown.promptId, {
      kind: 'approval',
      allow,
      remember: remember.value,
      access: allow ? grant.access : null,
      scopeTags: allow ? grant.scopeTags : null,
      defaultTag: allow ? grant.defaultTag : null,
    })
  } catch (error) {
    failure.value = errString(error)
  }
}

function onUpdateOpen(open: boolean) {
  if (!open) void answerAsync(false)
}
</script>

<template>
  <UiDrawerModal
    v-if="question"
    :open="true"
    :title="t('browserBridge.approval.title')"
    @update:open="onUpdateOpen"
  >
    <template #content>
      <div class="space-y-4" data-testid="bridge-approval">
        <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
          <dt class="text-muted-foreground">
            {{ t('browserBridge.client.name') }}
          </dt>
          <dd class="min-w-0 break-words" data-testid="bridge-approval-name">
            {{ question.client.clientName || t('browserBridge.client.noName') }}
            <span class="text-xs text-muted-foreground">
              ({{ t('browserBridge.client.claimed') }})
            </span>
          </dd>
          <dt class="text-muted-foreground">
            {{ t('browserBridge.client.fingerprint') }}
          </dt>
          <dd
            class="min-w-0 font-mono text-xs break-all"
            data-testid="bridge-approval-fingerprint"
          >
            {{ fingerprint }}
          </dd>
          <dt class="text-muted-foreground">
            {{ t('browserBridge.client.origin') }}
          </dt>
          <dd
            class="min-w-0 font-mono text-xs break-all"
            data-testid="bridge-approval-origin"
          >
            {{ question.client.origin }}
          </dd>
        </dl>
        <p class="text-sm">
          {{
            t('browserBridge.approval.declared', {
              access: t(`browserBridge.access.${question.declared.access}`),
              scope: question.declared.scopeTags
                ? question.declared.scopeTags.join(', ')
                : t('browserBridge.scope.all'),
            })
          }}
        </p>

        <BrowserBridgeGrantFields
          v-model="draft"
          :declared="question.declared"
          :declares-write="question.declaresWrite"
          test-prefix="bridge-approval"
        />

        <label class="flex items-center gap-2 text-sm">
          <ShadcnCheckbox
            v-model="remember"
            data-testid="bridge-approval-remember"
          />
          {{ t('browserBridge.approval.remember') }}
        </label>
        <p class="text-xs text-muted-foreground">
          {{
            remember
              ? t('browserBridge.approval.rememberedHint')
              : t('browserBridge.approval.sessionHint')
          }}
        </p>
        <p v-if="failure" class="text-sm text-destructive" role="alert">
          {{ failure }}
        </p>
      </div>
    </template>
    <template #footer>
      <div class="flex justify-end gap-2">
        <UiButton
          type="button"
          size="sm"
          variant="outline"
          data-testid="bridge-approval-deny"
          @click="answerAsync(false)"
        >
          {{ t('browserBridge.approval.deny') }}
        </UiButton>
        <UiButton
          type="button"
          size="sm"
          :disabled="!canAllow"
          data-testid="bridge-approval-allow"
          @click="answerAsync(true)"
        >
          {{ t('browserBridge.approval.allow') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
