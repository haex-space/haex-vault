<script setup lang="ts">
/**
 * The questions of the agent browser (spec 065, contracts/tauri-commands.md): may an agent open a
 * website (FR-007), and in Manuell, may it do something that changes a page. The agent's name is
 * its own claim; the address comes from the agent or the page. A website can be allowed once or
 * remembered for the agent, for reading or for operating (`http` only for reading, research R9).
 * Before something is sent to a website (FR-011, FR-012), the dialog lists what would be sent,
 * secrets as `•••`; the field names and values come from the page. A tab a page opens asks before
 * it loads anything (`newTab`); its address comes from the page. An agent can ask the user to take
 * over (`takeover`), with a reason in its own words. A download over 100 MB asks before the agent
 * gets it (`download`, FR-031), with its name from the website and its size. Closing the dialog
 * declines. A login fill (`fill`) shows the entries that match, by title, for the user only; the
 * user picks one and may remember the website for later fills (FR-021). A new login
 * (`newCredentials`, FR-036) shows the username the agent chose; "Speichern" stores the password
 * holzi generated and already entered, "Ablehnen" empties the fields again.
 */
import { BROWSER_ACTIONS } from '~/lib/actions/browserActions'
import { formatFileSize } from '~/lib/passwords/format'
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
const chosenEntry = ref<string | null>(null)
const allowLabel = computed(() => {
  switch (question.value?.kind) {
    case 'fill':
      return t('agentBrowser.prompt.fill.allow')
    case 'site':
      return t('agentBrowser.prompt.allowOnce')
    case 'newTab':
      return t('agentBrowser.prompt.newTab.allow')
    case 'takeover':
      return t('agentBrowser.prompt.takeover.allow')
    case 'download':
      return t('agentBrowser.prompt.download.allow')
    case 'newCredentials':
      return t('agentBrowser.prompt.newCredentials.allow')
    default:
      return t('agentBrowser.prompt.allow')
  }
})
const agentName = computed(
  () => question.value?.agent || t('agentBrowser.prompt.builtinAgent'),
)

watch(
  () => shown.value?.requestId,
  () => {
    failure.value = null
    const entries = question.value?.entries ?? []
    chosenEntry.value = entries.length === 1 ? (entries[0]?.id ?? null) : null
  },
  { immediate: true },
)

async function answerAsync(allow: boolean, remember?: Remember) {
  const asked = shown.value
  if (!asked) return
  const entry = asked.question.kind === 'fill' ? chosenEntry.value : null
  try {
    await store.answerAsync(
      asked.requestId,
      allow ? 'allow' : 'deny',
      remember,
      entry ?? undefined,
    )
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
                question.kind === 'site' || question.kind === 'newTab'
                  ? t('agentBrowser.prompt.address')
                  : question.kind === 'newCredentials'
                    ? t('agentBrowser.prompt.newCredentials.username')
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
          <template v-if="question.bytes !== undefined">
            <dt class="text-muted-foreground">
              {{ t('agentBrowser.prompt.download.size') }}
            </dt>
            <dd data-testid="agent-browser-prompt-size">
              {{ formatFileSize(question.bytes) }}
            </dd>
          </template>
          <template v-if="question.reason">
            <dt class="text-muted-foreground">
              {{ t('agentBrowser.prompt.takeover.reason') }}
            </dt>
            <dd
              class="min-w-0 break-words"
              data-testid="agent-browser-prompt-reason"
            >
              {{ question.reason }}
              <span class="text-xs text-muted-foreground">
                ({{ t('mcpAgents.registration.claimed') }})
              </span>
            </dd>
          </template>
        </dl>
        <fieldset
          v-if="question.kind === 'fill' && (question.entries?.length ?? 0) > 1"
          class="space-y-1"
          data-testid="agent-browser-prompt-entries"
        >
          <legend class="text-sm font-medium">
            {{ t('agentBrowser.prompt.fill.choose') }}
          </legend>
          <label
            v-for="entry in question.entries"
            :key="entry.id"
            class="flex items-center gap-2 text-sm"
          >
            <input
              v-model="chosenEntry"
              type="radio"
              name="agent-browser-entry"
              :value="entry.id"
            />
            {{ entry.title || t('agentBrowser.prompt.fill.untitled') }}
          </label>
        </fieldset>
        <p
          v-else-if="question.kind === 'fill' && question.entries?.[0]"
          class="text-sm"
          data-testid="agent-browser-prompt-entry"
        >
          {{ t('agentBrowser.prompt.fill.entry') }}:
          {{
            question.entries[0].title || t('agentBrowser.prompt.fill.untitled')
          }}
        </p>
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
          v-if="question.kind === 'fill'"
          type="button"
          size="sm"
          variant="outline"
          :disabled="!chosenEntry"
          data-testid="agent-browser-prompt-remember-fill"
          @click="answerAsync(true, 'fill')"
        >
          {{ t('agentBrowser.prompt.fill.remember') }}
        </UiButton>
        <UiButton
          type="button"
          size="sm"
          :disabled="question.kind === 'fill' && !chosenEntry"
          data-testid="agent-browser-prompt-allow"
          @click="answerAsync(true)"
        >
          {{ allowLabel }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
