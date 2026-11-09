<script setup lang="ts">
/**
 * "Cookies im Profil" in "KI → Browser" (spec 065 FR-046, research R17): the websites with
 * cookies and per cookie its path, expiry and flags, the value on a click, change and delete, and
 * delete all cookies of a website. Reading starts the agent browser when it is not running, so
 * the list is shown on a click unless it runs already. The window reads through commands no agent
 * can call; errors are toasts.
 */
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'vue-sonner'
import type { CookieView } from '~/types/bindings/CookieView'

const props = defineProps<{ running: boolean }>()

const { t } = useI18n()
const { errString } = useErrorString()
const deleteCookies = useActionOrThrow(
  'settings.agentBrowser.profile.cookie.delete',
)

const cookies = ref<CookieView[] | null>(null)
const loading = ref(false)
/** The website whose "alle löschen" waits for its confirmation. */
const confirming = ref<string | null>(null)

const sites = computed(() => {
  const bySite = new Map<string, CookieView[]>()
  for (const cookie of cookies.value ?? []) {
    bySite.set(cookie.site, [...(bySite.get(cookie.site) ?? []), cookie])
  }
  return [...bySite.entries()]
})

async function loadAsync() {
  loading.value = true
  try {
    cookies.value = await invoke<CookieView[]>('agent_browser_cookies')
  } catch (error) {
    toast.error(errString(error))
  } finally {
    loading.value = false
  }
}

async function deleteSiteAsync(site: string) {
  confirming.value = null
  try {
    await deleteCookies({ target: { site } })
  } catch (error) {
    toast.error(errString(error))
  }
  await loadAsync()
}

onMounted(() => {
  if (props.running) void loadAsync()
})
</script>

<template>
  <p class="px-1 text-sm font-medium">
    {{ t('agentBrowser.settings.cookies.title') }}
  </p>
  <SettingsGroup data-testid="agent-browser-cookies">
    <SettingsRow
      :title="t('agentBrowser.settings.cookies.title')"
      :description="
        cookies
          ? t('agentBrowser.settings.cookies.count', { n: cookies.length })
          : t('agentBrowser.settings.cookies.hint')
      "
    >
      <UiButton
        type="button"
        size="sm"
        variant="outline"
        :disabled="loading"
        data-testid="agent-browser-cookies-load"
        @click="loadAsync"
      >
        {{
          cookies
            ? t('agentBrowser.settings.cookies.reload')
            : t('agentBrowser.settings.cookies.load')
        }}
      </UiButton>
    </SettingsRow>
  </SettingsGroup>
  <template v-if="cookies">
    <SettingsGroup
      v-for="[site, list] in sites"
      :key="site"
      :label="site"
      data-testid="agent-browser-cookie-site"
      :data-site="site"
    >
      <SettingsRow
        :title="t('agentBrowser.settings.cookies.ofSite', { n: list.length })"
      >
        <template v-if="confirming === site">
          <UiButton
            type="button"
            size="sm"
            variant="outline"
            @click="confirming = null"
          >
            {{ t('agentBrowser.settings.cookies.cancel') }}
          </UiButton>
          <UiButton
            type="button"
            size="sm"
            variant="destructive"
            :data-testid="`agent-browser-cookie-site-confirm-${site}`"
            @click="deleteSiteAsync(site)"
          >
            {{ t('agentBrowser.settings.cookies.deleteSite') }}
          </UiButton>
        </template>
        <UiButton
          v-else
          type="button"
          size="sm"
          variant="outline"
          :data-testid="`agent-browser-cookie-site-delete-${site}`"
          @click="confirming = site"
        >
          {{ t('agentBrowser.settings.cookies.deleteSite') }}
        </UiButton>
      </SettingsRow>
      <SettingsAgentBrowserProfileCookieRow
        v-for="cookie in list"
        :key="`${cookie.domain}|${cookie.path}|${cookie.name}`"
        :cookie="cookie"
        @changed="loadAsync"
      />
    </SettingsGroup>
  </template>
</template>
