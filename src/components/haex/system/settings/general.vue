<template>
  <div class="h-full">
    <!-- Subviews -->
    <HaexSystemSettingsGeneralBasic v-if="activeView === 'basic'" @back="goBack" />
    <HaexSystemSettingsGeneralAppearance v-else-if="activeView === 'appearance'" @back="goBack" />

    <!-- Index / Menu -->
    <HaexSystemSettingsLayout v-else :title="t('title')">
      <div class="space-y-1">
        <HaexSystemSettingsLayoutMenuItem
          :label="t('menu.basic')"
          :description="t('menu.basicDesc')"
          icon="i-lucide-settings"
          @click="navigateTo('basic')"
        />
        <HaexSystemSettingsLayoutMenuItem
          :label="t('menu.appearance')"
          :description="t('menu.appearanceDesc')"
          icon="i-lucide-palette"
          @click="navigateTo('appearance')"
        />
      </div>
    </HaexSystemSettingsLayout>
  </div>
</template>

<script setup lang="ts">
const { t } = useI18n()
const tabId = inject<string>('haex-tab-id')!
const { activeView, navigateTo, goBack } = useDrillDownNavigation<'index' | 'basic' | 'appearance'>('index', 'general', tabId)
</script>

<i18n lang="yaml">
de:
  title: Allgemein
  menu:
    basic: Grundeinstellungen
    basicDesc: Sprache, Vaultname, Benachrichtigungen, Icons, Passwort
    appearance: Erscheinungsbild
    appearanceDesc: Design, Hintergrund, Gradient
en:
  title: General
  menu:
    basic: Basic Settings
    basicDesc: Language, vault name, notifications, icons, password
    appearance: Appearance
    appearanceDesc: Design, background, gradient
</i18n>
