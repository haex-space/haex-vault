<template>
  <div class="space-y-4">
    <!-- Server URL Selection -->
    <div class="flex flex-col space-y-2">
      <USelectMenu
        v-model="selectedServerOption"
        :items
        size="xl"
        class="w-full"
      >
        <template #item="{ item }">
          <UUser
            :name="item.label"
            :description="item.value"
          />
        </template>
      </USelectMenu>

      <UiInput
        v-if="selectedServerOption.value === 'custom'"
        v-model="customServerUrl"
        :label="t('customUrl.label')"
        size="xl"
        class="w-full"
      />
    </div>

    <UiInput
      v-model="email"
      type="email"
      :label="t('email.label')"
      leading-icon="i-lucide-mail"
      size="xl"
      class="w-full"
    />

    <UiInputPassword
      v-model="password"
      :label="t('password.label')"
      leading-icon="i-lucide-lock"
      size="xl"
      class="w-full"
    />
  </div>
</template>

<script setup lang="ts">
const { t } = useI18n()

defineProps<{
  items: ISyncServerOption[]
}>()

const serverUrl = defineModel<string>('serverUrl')
const email = defineModel<string>('email')
const password = defineModel<string>('password')

// Predefined server options

// Default server option
const defaultServerOption: ISyncServerOption = {
  label: 'HaexSpace',
  value: 'https://sync.haex.space',
}

// Form state
const selectedServerOption = ref<ISyncServerOption>(defaultServerOption)

const customServerUrl = ref()

watch(
  [customServerUrl, selectedServerOption],
  () => {
    if (selectedServerOption.value.value === 'custom') {
      serverUrl.value = customServerUrl.value
    } else {
      customServerUrl.value = ''
      serverUrl.value = selectedServerOption.value.value
    }
  },
  { immediate: true },
)
</script>

<i18n lang="yaml">
de:
  serverUrl:
    label: Server-URL
    description: Wähle einen vorkonfigurierten Server oder gib eine benutzerdefinierte URL ein
  customUrl:
    label: Benutzerdefinierte Server-URL
    description: Gib die URL deines eigenen Sync-Servers ein
    placeholder: https://dein-server.de
  email:
    label: E-Mail
    description: Deine E-Mail-Adresse für die Anmeldung
    placeholder: beispiel{'@'}email.de
  password:
    label: Passwort
    description: Dein Passwort für die Anmeldung
    placeholder: Passwort eingeben
  actions:
    connect: Verbinden
    cancel: Abbrechen

en:
  serverUrl:
    label: Server URL
    description: Choose a preconfigured server or enter a custom URL
  customUrl:
    label: Custom Server URL
    description: Enter the URL of your own sync server
    placeholder: https://your-server.com
  email:
    label: Email
    description: Your email address for authentication
    placeholder: example{'@'}email.com
  password:
    label: Password
    description: Your password for authentication
    placeholder: Enter password
  actions:
    connect: Connect
    cancel: Cancel
</i18n>
