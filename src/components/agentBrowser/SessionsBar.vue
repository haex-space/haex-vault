<script setup lang="ts">
/**
 * The sessions bar (spec 065, FR-015, FR-016, FR-017): while an agent steers the agent browser,
 * the desktop shows which agent, its tabs with website and title, and its last action, with
 * "Eingreifen" and "Stoppen". While the user has the browser, it says so (with the agent's reason
 * when the agent asked, as its claim) and offers "Zurückgeben". Stopping here is binding; the
 * marker in the browser tab is the convenient way. The agent's name is its own claim, a tab's
 * title comes from the website.
 */
import { BROWSER_ACTIONS } from '~/lib/actions/browserActions'
import type { BrowserSessionView } from '~/types/bindings/BrowserSessionView'

const { t } = useI18n()
const { errString } = useErrorString()
const store = useAgentBrowserStore()
const failure = ref<string | null>(null)

function agentName(session: BrowserSessionView): string {
  if (session.builtin) return t('agentBrowser.prompt.builtinAgent')
  return session.agent || t('agentBrowser.prompt.agent')
}

function tabsText(session: BrowserSessionView): string {
  if (session.tabs.length === 0) return t('agentBrowser.sessions.noTab')
  return session.tabs
    .map((tab) => [tab.tab, tab.site, tab.title].filter(Boolean).join(' · '))
    .join(', ')
}

function lastAction(session: BrowserSessionView): string | null {
  const last = session.lastAction
  if (!last) return null
  const def = BROWSER_ACTIONS.find((action) => action.id === last.tool)
  const action = def ? t(def.titleKey) : last.tool
  return t('agentBrowser.sessions.lastAction', {
    action: last.site ? `${action} · ${last.site}` : action,
  })
}

function pauseText(session: BrowserSessionView): string | null {
  const pause = session.paused
  if (!pause) return null
  if (pause.by === 'request')
    return t('agentBrowser.sessions.requested', { reason: pause.reason })
  return t('agentBrowser.sessions.paused')
}

async function runAsync(action: () => Promise<void>) {
  failure.value = null
  try {
    await action()
  } catch (error) {
    failure.value = errString(error)
  }
}

function stopAsync(session: BrowserSessionView) {
  return runAsync(() => store.stopAsync(session.id))
}

function steerAsync(session: BrowserSessionView) {
  return runAsync(() =>
    store.steerAsync(session.id, session.paused ? 'return' : 'takeover'),
  )
}
</script>

<template>
  <div
    v-if="store.sessions.length > 0"
    class="pointer-events-none absolute inset-x-0 top-2 z-40 flex justify-center px-4"
  >
    <section
      class="pointer-events-auto flex max-w-full flex-col gap-1 rounded-xl border border-border bg-background/95 px-3 py-2 shadow-lg"
      :aria-label="t('agentBrowser.sessions.label')"
      data-testid="agent-browser-sessions"
    >
      <div
        v-for="session in store.sessions"
        :key="session.id"
        class="flex min-w-0 items-center gap-3"
        data-testid="agent-browser-session"
        :data-id="session.id"
      >
        <Icon
          name="lucide:globe"
          class="size-4 shrink-0 text-primary"
          :aria-hidden="true"
        />
        <div class="min-w-0 flex-1 text-sm">
          <p class="truncate font-medium">
            {{ agentName(session) }}
            <span
              v-if="!session.builtin"
              class="text-xs font-normal text-muted-foreground"
            >
              ({{ t('mcpAgents.registration.claimed') }})
            </span>
          </p>
          <p
            class="truncate text-xs text-muted-foreground"
            data-testid="agent-browser-session-tabs"
          >
            {{ tabsText(session) }}
          </p>
          <p
            v-if="lastAction(session)"
            class="truncate text-xs text-muted-foreground"
          >
            {{ lastAction(session) }}
          </p>
          <p
            v-if="pauseText(session)"
            class="max-w-md text-xs break-words text-warning"
            data-testid="agent-browser-session-paused"
          >
            {{ pauseText(session) }}
          </p>
        </div>
        <UiButton
          type="button"
          size="sm"
          :variant="session.paused ? 'default' : 'outline'"
          data-testid="agent-browser-session-steer"
          @click="steerAsync(session)"
        >
          {{
            session.paused
              ? t('agentBrowser.sessions.giveBack')
              : t('agentBrowser.sessions.takeOver')
          }}
        </UiButton>
        <UiButton
          type="button"
          size="sm"
          variant="destructive"
          data-testid="agent-browser-session-stop"
          @click="stopAsync(session)"
        >
          {{ t('agentBrowser.sessions.stop') }}
        </UiButton>
      </div>
      <p v-if="failure" class="text-xs text-destructive" role="alert">
        {{ failure }}
      </p>
    </section>
  </div>
</template>
