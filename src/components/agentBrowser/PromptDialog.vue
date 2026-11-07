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
 * gets it (`download`, FR-031), with its name from the website and its size. Only the dialog's
 * buttons answer (FR-042). A login fill (`fill`) shows the entries that match, by title, for the user only; the
 * user picks one and may remember the website for later fills (FR-021). An entry fill
 * (`entryFill`, FR-037) shows the agent's purpose as its statement, the entries to choose from and
 * one row per mapped field with its value hidden; the user deselects rows and is asked every time.
 * A request the page sends (`request`, FR-044) shows the website it goes to and
 * what it sends; one checkbox "Für diese Website merken" turns the answer into a rule. A new login
 * (`newCredentials`, FR-036) shows the username the agent chose; "Speichern" stores the password
 * holzi generated and already entered, "Ablehnen" empties the fields again.
 * What "Für diese Website merken" remembers goes into the agent profile of the agent (spec 083), so
 * the question says "gilt für alle Agenten im Agentenprofil {name}" with their number.
 */
import { toast } from 'vue-sonner'
import { BROWSER_ACTIONS } from '~/lib/actions/browserActions'
import { formatFileSize } from '~/lib/passwords/format'
import type { Remember } from '~/types/bindings/Remember'

const { t } = useI18n()
const { errString } = useErrorString()
const store = useAgentBrowserStore()

const shown = computed(() => store.shown)
const question = computed(() => shown.value?.question ?? null)
const waiting = computed(() => Math.max(store.prompts.length - 1, 0))
const plainHttp = computed(
  () => question.value?.site?.startsWith('http://') === true,
)
const toolTitle = computed(() => {
  const tool = question.value?.tool
  const def = BROWSER_ACTIONS.find((action) => action.id === tool)
  return def ? t(def.titleKey) : tool
})
/** "Für diese Website merken": one checkbox in every dialog that can remember (FR-007, FR-021, FR-044). */
const remember = ref(false)
/** What a remembered website may do (FR-007); `http` can only be read (research R9). */
const rememberLevel = ref<'site-read' | 'site-operate'>('site-operate')
/** Only a question with its website can remember anything for that website. */
const canRemember = computed(
  () =>
    !!question.value?.site &&
    ['site', 'fill', 'request'].includes(question.value.kind),
)
/**
 * What an answer remembers. A website and a login fill remember only an allowance; a page request
 * remembers either answer as a rule for the website it goes to.
 */
function remembered(allow: boolean): Remember | undefined {
  if (!canRemember.value || !remember.value) return undefined
  switch (question.value?.kind) {
    case 'request':
      return 'request'
    case 'fill':
      return allow ? 'fill' : undefined
    case 'site':
      return allow
        ? plainHttp.value
          ? 'site-read'
          : rememberLevel.value
        : undefined
    default:
      return undefined
  }
}
const chosenEntry = ref<string | null>(null)
/** The rows of an entry fill the user deselected. */
const skipped = ref<Set<string>>(new Set())
const choosesEntry = computed(
  () => question.value?.kind === 'fill' || question.value?.kind === 'entryFill',
)
const chosenRows = computed(() =>
  (question.value?.mapping ?? []).filter(
    (row) => row.entry === chosenEntry.value,
  ),
)

/** The entry's field as the user knows it: the standard fields by their names. */
function keyLabel(key: string): string {
  if (key === 'username' || key === 'password')
    return t(`passwords.fields.${key}`)
  if (key === 'otp') return t('agentBrowser.prompt.entryFill.otp')
  return key
}

function toggleRow(id: string, keep: boolean) {
  const next = new Set(skipped.value)
  if (keep) next.delete(id)
  else next.add(id)
  skipped.value = next
}
const allowLabel = computed(() => {
  switch (question.value?.kind) {
    case 'fill':
      return t('agentBrowser.prompt.allow')
    case 'site':
      return t('agentBrowser.prompt.allow')
    case 'newTab':
      return t('agentBrowser.prompt.newTab.allow')
    case 'takeover':
      return t('agentBrowser.prompt.takeover.allow')
    case 'download':
      return t('agentBrowser.prompt.download.allow')
    case 'entryFill':
      return t('agentBrowser.prompt.entryFill.allow')
    case 'newCredentials':
      return t('agentBrowser.prompt.newCredentials.allow')
    case 'switchOn':
      return t('agentBrowser.prompt.switchOn.allow')
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
    const entries = question.value?.entries ?? []
    chosenEntry.value = entries.length === 1 ? (entries[0]?.id ?? null) : null
    skipped.value = new Set()
    remember.value = false
    rememberLevel.value = plainHttp.value ? 'site-read' : 'site-operate'
  },
  { immediate: true },
)

/** Answers with the dialog's buttons; the checkbox says what the answer remembers. */
async function answerAsync(allow: boolean, remembering?: Remember) {
  const asked = shown.value
  if (!asked) return
  const kind = asked.question.kind
  const entry =
    kind === 'fill' || kind === 'entryFill' ? chosenEntry.value : null
  const skip =
    kind === 'entryFill'
      ? chosenRows.value
          .filter((row) => skipped.value.has(row.id))
          .map((row) => row.id)
      : undefined
  try {
    await store.answerAsync(
      asked.requestId,
      allow ? 'allow' : 'deny',
      remembering,
      entry ?? undefined,
      skip,
    )
  } catch (error) {
    toast.error(errString(error))
  }
}
</script>

<template>
  <UiDrawerModal
    v-if="question"
    :open="true"
    :title="t(`agentBrowser.prompt.${question.kind}.title`)"
    :dismissible="false"
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
              {{
                question.kind === 'entryFill'
                  ? t('agentBrowser.prompt.entryFill.purpose')
                  : t('agentBrowser.prompt.takeover.reason')
              }}
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
          v-if="choosesEntry && (question.entries?.length ?? 0) > 1"
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
          v-else-if="choosesEntry && question.entries?.[0]"
          class="text-sm"
          data-testid="agent-browser-prompt-entry"
        >
          {{ t('agentBrowser.prompt.fill.entry') }}:
          {{
            question.entries[0].title || t('agentBrowser.prompt.fill.untitled')
          }}
        </p>
        <fieldset
          v-if="question.kind === 'entryFill' && chosenRows.length > 0"
          class="space-y-1"
          data-testid="agent-browser-prompt-mapping"
        >
          <legend class="text-sm font-medium">
            {{ t('agentBrowser.prompt.entryFill.rows') }}
          </legend>
          <label
            v-for="row in chosenRows"
            :key="row.id"
            class="flex items-center gap-2 text-sm"
            :data-testid="`agent-browser-prompt-row-${row.id}`"
          >
            <input
              type="checkbox"
              :checked="!skipped.has(row.id)"
              @change="
                toggleRow(row.id, ($event.target as HTMLInputElement).checked)
              "
            />
            <span class="min-w-0 break-words">{{ row.field }}</span>
            <span class="text-muted-foreground">←</span>
            <span class="min-w-0 break-words">{{ keyLabel(row.key) }}</span>
            <span
              v-if="row.frame"
              class="min-w-0 break-all text-xs text-muted-foreground"
              >({{
                t('agentBrowser.prompt.entryFill.frame', { site: row.frame })
              }})</span
            >
            <span class="text-muted-foreground">•••</span>
          </label>
        </fieldset>
        <div v-if="question.fields.length > 0" class="space-y-1">
          <p class="text-sm font-medium">
            {{ t('agentBrowser.prompt.submit.fields') }}
          </p>
          <dl
            class="grid max-h-48 grid-cols-[auto_1fr] gap-x-3 gap-y-1 overflow-y-auto rounded-md border border-border p-2 text-xs"
            data-testid="agent-browser-prompt-fields"
          >
            <template v-for="(field, index) in question.fields" :key="index">
              <dt class="break-all text-muted-foreground">
                {{ field.name || t('agentBrowser.prompt.submit.body') }}
              </dt>
              <dd class="min-w-0 whitespace-pre-wrap break-all">
                {{ field.value }}
              </dd>
            </template>
          </dl>
        </div>
        <div v-if="canRemember" class="space-y-1">
          <label class="flex items-center gap-2 text-sm">
            <ShadcnCheckbox
              v-model="remember"
              data-testid="agent-browser-prompt-remember"
            />
            {{ t(`agentBrowser.prompt.${question.kind}.remember`) }}
          </label>
          <fieldset
            v-if="question.kind === 'site' && remember && !plainHttp"
            class="space-y-1 pl-6"
            data-testid="agent-browser-prompt-remember-level"
          >
            <label
              v-for="level in ['site-operate', 'site-read'] as const"
              :key="level"
              class="flex items-center gap-2 text-sm"
            >
              <input
                v-model="rememberLevel"
                type="radio"
                name="agent-browser-remember-level"
                :value="level"
              />
              {{ t(`agentBrowser.prompt.site.${level}`) }}
            </label>
          </fieldset>
        </div>
        <p class="text-sm text-muted-foreground">
          {{ t(`agentBrowser.prompt.${question.kind}.hint`) }}
        </p>
        <p v-if="waiting > 0" class="text-xs text-muted-foreground">
          {{ t('agentBrowser.prompt.more', { count: waiting }) }}
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
          @click="answerAsync(false, remembered(false))"
        >
          {{ t('agentBrowser.prompt.deny') }}
        </UiButton>
        <UiButton
          type="button"
          size="sm"
          :disabled="choosesEntry && !chosenEntry"
          data-testid="agent-browser-prompt-allow"
          @click="answerAsync(true, remembered(true))"
        >
          {{ allowLabel }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
