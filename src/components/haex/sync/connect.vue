<template>
  <div class="space-y-4">
    <!-- Server URL Selection -->
    <UFormField
      :label="t('serverUrl.label')"
      :description="t('serverUrl.description')"
    >
      <USelectMenu
        v-model="selectedServerOption"
        :options="serverOptions"
        value-attribute="value"
        option-attribute="label"
        size="xl"
        class="w-full"
        @update:model-value="onServerOptionChange"
      />
    </UFormField>

    <!-- Custom Server URL Input (shown when "Custom" is selected) -->
    <UFormField
      v-if="isCustomServer"
      :label="t('customUrl.label')"
      :description="t('customUrl.description')"
    >
      <UiInput
        v-model="customServerUrl"
        :placeholder="t('customUrl.placeholder')"
        size="xl"
        class="w-full"
      />
    </UFormField>

    <!-- Email Input -->
    <UFormField
      :label="t('email.label')"
      :description="t('email.description')"
    >
      <UiInput
        v-model="email"
        type="email"
        :placeholder="t('email.placeholder')"
        leading-icon="i-lucide-mail"
        size="xl"
        class="w-full"
      />
    </UFormField>

    <!-- Password Input -->
    <UFormField
      :label="t('password.label')"
      :description="t('password.description')"
    >
      <UiInputPassword
        v-model="password"
        :placeholder="t('password.placeholder')"
        leading-icon="i-lucide-lock"
        size="xl"
        class="w-full"
      />
    </UFormField>
  </div>
</template>

<script setup lang="ts">
const { t } = useI18n()

interface ServerOption {
  label: string
  value: string
}

defineProps<{
  isLoading?: boolean
  showCancel?: boolean
}>()

const emit = defineEmits<{
  update: [{ serverUrl: string; email: string; password: string }]
}>()

// Predefined server options
const serverOptions = computed<ServerOption[]>(() => [
  {
    label: 'HaexSpace (sync.haex.space)',
    value: 'https://sync.haex.space',
  },
  {
    label: t('serverOptions.localhost'),
    value: 'http://localhost:3002',
  },
  {
    label: t('serverOptions.custom'),
    value: 'custom',
  },
])

// Default server option
const defaultServerOption: ServerOption = {
  label: 'HaexSpace (sync.haex.space)',
  value: 'https://sync.haex.space',
}

// Form state
const selectedServerOption = ref<ServerOption>(defaultServerOption)
const customServerUrl = ref('')
const email = ref('')
const password = ref('')

// Computed
const isCustomServer = computed(
  () => selectedServerOption.value.value === 'custom',
)

const currentServerUrl = computed(() => {
  if (isCustomServer.value) {
    return customServerUrl.value
  }
  return selectedServerOption.value.value
})

// Methods
const onServerOptionChange = () => {
  // Clear custom URL when switching away from custom option
  if (!isCustomServer.value) {
    customServerUrl.value = ''
  }
}

// Watch for changes and emit update
watch(
  [selectedServerOption, customServerUrl, email, password],
  () => {
    emit('update', {
      serverUrl: currentServerUrl.value,
      email: email.value,
      password: password.value,
    })
  },
)

// Expose method to clear form
const clearForm = () => {
  selectedServerOption.value = defaultServerOption
  customServerUrl.value = ''
  email.value = ''
  password.value = ''
}

defineExpose({
  clearForm,
})
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
    placeholder: beispiel at email.de
  password:
    label: Passwort
    description: Dein Passwort für die Anmeldung
    placeholder: Passwort eingeben
  serverOptions:
    localhost: Lokal (localhost:3002)
    custom: Benutzerdefiniert...
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
    placeholder: example at email.com
  password:
    label: Password
    description: Your password for authentication
    placeholder: Enter password
  serverOptions:
    localhost: Local (localhost:3002)
    custom: Custom...
  actions:
    connect: Connect
    cancel: Cancel
</i18n>
