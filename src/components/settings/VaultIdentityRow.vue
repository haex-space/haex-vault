<script setup lang="ts">
/**
 * The vault's public identity (spec 024, FR-046): the key that names the vault in member lists and
 * rights of spaces and shares, as `npub`; not an address to invite the vault at. It can be copied
 * and not changed; the private key is shown nowhere (FR-002).
 */
import { toast } from 'vue-sonner'
import type { VaultPublicIdentity } from '@bindings/VaultPublicIdentity'

defineProps<{
  identity: VaultPublicIdentity
}>()

const { t } = useI18n()
const { errString } = useErrorString()
</script>

<template>
  <SettingsGroup :label="t('settings.federation.identityLabel')">
    <SettingsRow
      icon="lucide:key-round"
      :title="t('settings.federation.identityTitle')"
      :description="t('settings.federation.identityDescription')"
    >
      <template #default>
        <UiButtonCopy
          size="icon-sm"
          :labels="{
            copy: t('settings.federation.identityCopy'),
            copied: t('settings.federation.identityCopied'),
          }"
          :value="identity.npub"
          data-testid="settings-identity-copy"
          @error="toast.error(errString($event))"
        />
      </template>
    </SettingsRow>
    <li class="px-4 py-3">
      <output
        class="block font-mono text-xs break-all select-all"
        data-testid="settings-identity-npub"
      >
        {{ identity.npub }}
      </output>
    </li>
  </SettingsGroup>
</template>
