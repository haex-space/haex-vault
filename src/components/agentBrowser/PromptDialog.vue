<script setup lang="ts">
/**
 * The questions of the agent browser (spec 065, contracts/tauri-commands.md): may an agent open a
 * website (FR-007), and in Manuell, may it do something that changes a page. The agent's name is
 * its own claim; the address comes from the agent or the page. A website can be allowed once or
 * remembered for the agent, for reading or for operating (`http` only for reading, research R9).
 * Before something is sent to a website (FR-011, FR-012), the dialog lists what would be sent,
 * secrets as `•••`; the field names and values come from the page. Closing the dialog declines.
 */
import { BROWSER_ACTIONS } from '~/lib/actions/browserActions'
import type { Remember } from '~/types/bindings/Remember'

const { t } = useI18n()
const { errString } = useErrorString()
const store = useAgentBrowserStore()

const shown = computed(() => store.shown)
const question = computed(() => shown.value?.question ?? null)
const failure = ref<string | null>(null)
const waiting = computed(() => Math.max(store.prompts.length - 1, 0))
const plainHttp = computed(
  () => question.value?.site?.startsWith('http://') === true,
)
const toolTitle = computed(() => {
  const tool = question.value?.tool
  const def = BROWSER_ACTIONS.find((action) => action.id === tool)
  return def ? t(def.titleKey) : tool
})
const agentName = computed(
  () => question.value?.agent || t('agentBrowser.prompt.builtinAgent'),
)

watch(
  () => shown.value?.requestId,
  () => {
    failure.value = null
  },
)

async function answerAsync(allow: boolean, remember?: Remember) {
  const asked = shown.value
  if (!asked) return
  try {
    await store.answerAsync(asked.requestId, allow ? 'allow' : 'deny', remember)
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
    :title="t(`agentBrowser.prompt.${question.kind}.title`)"
    @update:open="onUpdateOpen"
  >
    <template #content>
      <div
        class="space-y-4"
        data-testid="agent-browser-prompt"
        :data-kind="question.kind"
      >
        <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
          <dt class="text-muted-foreground">
            {{ t('agentBrowser.prompt.agent') }}
          </dt>
          <dd class="min-w-0 break-words">
            {{ agentName }}
            <span v-if="question.agent" class="text-xs text-muted-foreground">
              ({{ t('mcpAgents.registration.claimed') }})
            </span>
          </dd>
          <template v-if="question.site">
            <dt class="text-muted-foreground">
              {{ t('agentBrowser.prompt.siteLabel') }}
            </dt>
            <dd
              class="min-w-0 font-medium break-words"
              data-testid="agent-browser-prompt-site"
            >
              {{ question.site }}
            </dd>
          </template>
          <template v-if="question.tool">
            <dt class="text-muted-foreground">
              {{ t('agentBrowser.prompt.actionLabel') }}
            </dt>
            <dd class="min-w-0 break-words">{{ toolTitle }}</dd>
          </template>
          <template v-if="question.label">
            <dt class="text-muted-foreground">
              {{
                question.kind === 'site'
                  ? t('agentBrowser.prompt.address')
                  : t('agentBrowser.prompt.target')
              }}
            </dt>
            <dd
              class="min-w-0 break-all text-xs"
              data-testid="agent-browser-prompt-label"
            >
              {{ question.label }}
            </dd>
          </template>
        </dl>
        <div v-if="question.fields.length > 0" class="space-y-1">
          <p class="text-sm font-medium">
            {{ t('agentBrowser.prompt.submit.fields') }}
          </p>
          <dl
            class="grid max-h-48 grid-cols-[auto_1fr] gap-x-3 gap-y-1 overflow-y-auto rounded-md border border-border p-2 text-xs"
            data-testid="agent-browser-prompt-fields"
          >
            <template v-for="(field, index) in question.fields" :key="index">
              <dt class="break-all text-muted-foreground">{{ field.name }}</dt>
              <dd class="min-w-0 break-all">{{ field.value }}</dd>
            </template>
          </dl>
        </div>
        <p class="text-sm text-muted-foreground">
          {{ t(`agentBrowser.prompt.${question.kind}.hint`) }}
        </p>
        <p v-if="waiting > 0" class="text-xs text-muted-foreground">
          {{ t('agentBrowser.prompt.more', { count: waiting }) }}
        </p>
        <p v-if="failure" class="text-sm text-destructive" role="alert">
          {{ failure }}
        </p>
      </div>
    </template>
    <template #footer>
      <div class="flex flex-wrap justify-end gap-2">
        <UiButton
          type="button"
          size="sm"
          variant="outline"
          data-testid="agent-browser-prompt-deny"
          @click="answerAsync(false)"
        >
          {{ t('agentBrowser.prompt.deny') }}
        </UiButton>
        <template v-if="question.kind === 'site'">
          <UiButton
            type="button"
            size="sm"
            variant="outline"
            data-testid="agent-browser-prompt-remember-read"
            @click="answerAsync(true, 'site-read')"
          >
            {{ t('agentBrowser.prompt.rememberRead') }}
          </UiButton>
          <UiButton
            v-if="!plainHttp"
            type="button"
            size="sm"
            variant="outline"
            data-testid="agent-browser-prompt-remember-operate"
            @click="answerAsync(true, 'site-operate')"
          >
            {{ t('agentBrowser.prompt.rememberOperate') }}
          </UiButton>
        </template>
        <UiButton
          type="button"
          size="sm"
          data-testid="agent-browser-prompt-allow"
          @click="answerAsync(true)"
        >
          {{
            question.kind === 'site'
              ? t('agentBrowser.prompt.allowOnce')
              : t('agentBrowser.prompt.allow')
          }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
