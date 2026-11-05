<script setup lang="ts">
/**
 * The rights group "Browser" of an agent (spec 065 FR-005, data-model.md): rows for a
 * `SettingsGroup` of the rights list. The row has the levels of every other right and picks what
 * the agent may do on every website: nothing (the group is off), read, or operate (FR-043);
 * "Fragt bei neuen Websites" is the group on without a level for all websites, where only listed
 * websites open without a question. Once it is on, the exceptions follow inside the same list:
 * websites that are blocked, read only or operated whatever the level for all says (FR-047), each
 * with its own level. A website over plain http can only be read (research R9). A website where
 * the user chose "Für diese Website merken" for a login fill shows it and can forget it
 * (FR-021). Rust checks every change again.
 */
import { siteOf } from '~/lib/agents/rights'
import type { BrowserRights } from '~/types/bindings/BrowserRights'
import type { SiteLevel } from '~/types/bindings/SiteLevel'

const props = defineProps<{
  modelValue: BrowserRights | null
  disabled?: boolean
}>()
const emit = defineEmits<{ 'update:modelValue': [BrowserRights | null] }>()

const { t } = useI18n()
const fieldLabels = useFieldLabels()

const draft = ref('')
const problem = ref(false)
// The hint under the field goes away as soon as the typed address is valid (or the field empty).
watch(draft, (value) => {
  if (problem.value) problem.value = !!value.trim() && siteOf(value) === null
})

const sites = computed(() =>
  Object.entries(props.modelValue?.sites ?? {}).sort(([a], [b]) =>
    a.localeCompare(b),
  ),
)

type BrowserChoice = 'none' | 'ask' | 'read' | 'write'

const choices = computed(() =>
  (['none', 'ask', 'read', 'write'] as const).map((value) => ({
    value,
    label:
      value === 'ask'
        ? t('mcpAgents.browser.ask')
        : t(`mcpAgents.level.${value}`),
  })),
)

const choice = computed<BrowserChoice>(() => {
  const rights = props.modelValue
  if (!rights) return 'none'
  if (!rights.all) return 'ask'
  return rights.all === 'operate' ? 'write' : 'read'
})

const siteLevels = computed(() =>
  (['blocked', 'read', 'operate'] as const).map((value) => ({
    value,
    label: t(`mcpAgents.browser.level.${value}`),
  })),
)

/** A website over plain http can only be blocked or read (research R9). */
const plainLevels = computed(() =>
  siteLevels.value.filter((option) => option.value !== 'operate'),
)

function setChoice(value: string) {
  if (value === 'none') {
    emit('update:modelValue', null)
    return
  }
  const all: SiteLevel | null =
    value === 'ask' ? null : value === 'write' ? 'operate' : 'read'
  const current = props.modelValue ?? {
    userBrowser: false,
    sites: {},
    requests: {},
  }
  emit('update:modelValue', { ...current, all })
}

function change(next: Partial<BrowserRights>) {
  if (!props.modelValue) return
  emit('update:modelValue', { ...props.modelValue, ...next })
}

function setLevel(site: string, level: string) {
  const current = props.modelValue?.sites[site]
  if (!current) return
  change({
    sites: {
      ...props.modelValue?.sites,
      [site]: { ...current, level: level as SiteLevel },
    },
  })
}

function forgetFill(site: string) {
  const current = props.modelValue?.sites[site]
  if (!current) return
  change({
    sites: {
      ...props.modelValue?.sites,
      [site]: { ...current, fillRemembered: false },
    },
  })
}

const requests = computed(() =>
  Object.entries(props.modelValue?.requests ?? {}).sort(([a], [b]) =>
    a.localeCompare(b),
  ),
)

function forgetRequest(site: string) {
  const { [site]: _removed, ...rest } = props.modelValue?.requests ?? {}
  change({ requests: rest })
}

function remove(site: string) {
  const { [site]: _removed, ...rest } = props.modelValue?.sites ?? {}
  change({ sites: rest })
}

/** A website typed here is blocked; its level can change in its row. */
function add() {
  const site = siteOf(draft.value)
  problem.value = site === null
  if (!site) return
  const level: SiteLevel = 'blocked'
  change({
    sites: {
      ...props.modelValue?.sites,
      [site]: props.modelValue?.sites[site] ?? { level, fillRemembered: false },
    },
  })
  draft.value = ''
}

function plain(site: string): boolean {
  return site.startsWith('http://')
}
</script>

<template>
  <SettingsRow
    :title="t('mcpAgents.browser.title')"
    :description="t('mcpAgents.browser.enabledHint')"
    label-for="mcp-browser-level"
  >
    <SettingsSelect
      id="mcp-browser-level"
      :model-value="choice"
      class="w-52"
      :options="choices"
      :disabled="disabled"
      data-testid="mcp-browser-level"
      @update:model-value="setChoice"
    />
  </SettingsRow>
  <AgentsRightsInset v-if="modelValue" data-testid="mcp-browser-exceptions">
    <SettingsGroup :label="t('mcpAgents.browser.exceptions')">
      <SettingsRow
        v-for="[site, grant] in sites"
        :key="site"
        :title="site"
        data-testid="mcp-browser-site"
        :data-site="site"
      >
        <div class="flex flex-wrap items-center gap-2">
          <UiButton
            v-if="grant.fillRemembered"
            type="button"
            size="sm"
            variant="outline"
            :disabled="disabled"
            :title="t('mcpAgents.browser.fillRememberedHint')"
            :data-testid="`mcp-browser-forget-fill-${site}`"
            @click="forgetFill(site)"
          >
            {{ t('mcpAgents.browser.forgetFill') }}
          </UiButton>
          <SettingsSelect
            :model-value="
              plain(site) && grant.level === 'operate' ? 'read' : grant.level
            "
            class="w-48"
            :options="plain(site) ? plainLevels : siteLevels"
            :disabled="disabled"
            :data-testid="`mcp-browser-level-${site}`"
            @update:model-value="setLevel(site, $event)"
          />
          <UiButton
            type="button"
            size="sm"
            variant="outline"
            :disabled="disabled"
            :aria-label="t('mcpAgents.browser.remove', { site })"
            @click="remove(site)"
          >
            {{ t('mcpAgents.browser.removeShort') }}
          </UiButton>
        </div>
      </SettingsRow>
      <SettingsRow
        v-if="sites.length === 0"
        :title="t('mcpAgents.browser.none')"
      />
      <SettingsRow
        v-for="[site, rule] in requests"
        :key="`request:${site}`"
        :title="t('mcpAgents.browser.requestRule', { site })"
        :description="t('mcpAgents.browser.requestHint')"
        data-testid="mcp-browser-request-rule"
        :data-site="site"
      >
        <div class="flex flex-wrap items-center gap-2">
          <span class="text-sm">
            {{ t(`mcpAgents.browser.request.${rule}`) }}
          </span>
          <UiButton
            type="button"
            size="sm"
            variant="outline"
            :disabled="disabled"
            :data-testid="`mcp-browser-forget-request-${site}`"
            @click="forgetRequest(site)"
          >
            {{ t('mcpAgents.browser.removeShort') }}
          </UiButton>
        </div>
      </SettingsRow>
    </SettingsGroup>
    <div class="flex flex-col gap-1">
      <div class="flex items-end gap-2">
        <UiInput
          id="mcp-browser-site"
          v-model="draft"
          class="flex-1"
          :label="t('mcpAgents.browser.add')"
          :labels="fieldLabels.input.value"
          label-bg="var(--background)"
          type="url"
          autocomplete="off"
          placeholder="https://shop.example.de"
          :disabled="disabled"
          data-testid="mcp-browser-new-site"
          @keydown.enter.prevent="add"
        />
        <UiButton
          type="button"
          :disabled="disabled || !draft.trim()"
          data-testid="mcp-browser-add"
          @click="add"
        >
          {{ t('mcpAgents.browser.addShort') }}
        </UiButton>
      </div>
      <p v-if="problem" class="text-xs text-destructive" role="alert">
        {{ t('mcpAgents.browser.invalid') }}
      </p>
      <p v-else class="text-xs text-muted-foreground">
        {{
          choice === 'ask'
            ? t('mcpAgents.browser.hintAsk')
            : t('mcpAgents.browser.hint')
        }}
      </p>
    </div>
  </AgentsRightsInset>
</template>
