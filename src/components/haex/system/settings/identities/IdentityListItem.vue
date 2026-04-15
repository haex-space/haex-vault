<template>
  <div class="p-3 rounded-lg border border-default">
    <UCollapsible
      :open="expanded"
      :unmount-on-hide="false"
      @update:open="(val: boolean) => $emit('toggle', val)"
    >
      <div class="flex items-center justify-between cursor-pointer">
        <div class="flex items-center gap-2 flex-1 min-w-0">
          <UIcon
            name="i-lucide-chevron-right"
            class="w-4 h-4 shrink-0 text-muted transition-transform duration-200"
            :class="{ 'rotate-90': expanded }"
          />
          <UiAvatar
            :src="identity.avatar"
            :seed="identity.did"
            :avatar-options="avatarOptions"
            avatar-style="toon-head"
            size="sm"
          />
          <span class="font-medium truncate">{{ identity.name }}</span>
        </div>

        <div
          class="shrink-0 ml-4"
          @click.stop
        >
          <!-- Large screens: inline buttons -->
          <div class="hidden @md:flex items-center gap-1">
            <UButton
              variant="ghost"
              icon="i-lucide-qr-code"
              :title="t('shareQr')"
              @click="$emit('share-qr')"
            />
            <UButton
              variant="ghost"
              icon="i-lucide-copy"
              :title="t('copyDid')"
              @click="$emit('copy-did')"
            />
            <UButton
              variant="ghost"
              icon="i-lucide-download"
              :title="t('export')"
              @click="$emit('export')"
            />
            <UButton
              variant="ghost"
              icon="i-lucide-pencil"
              :title="t('edit')"
              @click="$emit('edit')"
            />
            <UButton
              variant="ghost"
              color="error"
              icon="i-lucide-trash-2"
              :title="t('delete')"
              @click="$emit('delete')"
            />
          </div>
          <!-- Small screens: dropdown menu -->
          <UDropdownMenu
            class="@md:hidden"
            :items="menuItems"
          >
            <UButton
              variant="ghost"
              icon="i-lucide-ellipsis-vertical"
              color="neutral"
            />
          </UDropdownMenu>
        </div>
      </div>
      <template #content>
        <div class="mt-3 pt-3 border-t border-default space-y-3">
          <!-- DID Key -->
          <div class="flex items-center gap-2">
            <code class="text-xs text-muted truncate flex-1 min-w-0">{{
              identity.did
            }}</code>
            <UButton
              variant="ghost"
              icon="i-lucide-copy"
              size="xs"
              :title="t('copyDid')"
              @click="$emit('copy-did')"
            />
          </div>

          <!-- Claims -->
          <div class="flex flex-wrap items-center justify-between gap-2">
            <span class="text-sm font-medium">{{ t('claimsTitle') }}</span>
            <UButton
              variant="outline"
              icon="i-lucide-plus"
              @click="$emit('add-claim')"
            >
              {{ t('addClaim') }}
            </UButton>
          </div>

          <div
            v-if="claims.length"
            class="space-y-1"
          >
            <div
              v-for="claim in claims"
              :key="claim.id"
              class="flex flex-wrap items-center justify-between gap-2 p-2 rounded bg-gray-50 dark:bg-gray-800/50"
            >
              <div class="min-w-0 flex-1">
                <span class="text-xs font-medium text-muted">{{
                  claim.type
                }}</span>
                <p class="text-sm truncate">{{ claim.value }}</p>
              </div>
              <div class="flex gap-1 shrink-0">
                <UButton
                  variant="ghost"
                  icon="i-lucide-copy"
                  @click="$emit('copy-claim', claim.value)"
                />
                <UButton
                  variant="ghost"
                  icon="i-lucide-pencil"
                  @click="$emit('edit-claim', claim)"
                />
                <UButton
                  variant="ghost"
                  color="error"
                  icon="i-lucide-trash-2"
                  @click="$emit('delete-claim', claim.id)"
                />
              </div>
            </div>
          </div>
          <p
            v-else
            class="text-xs text-muted"
          >
            {{ t('claimsEmpty') }}
          </p>
        </div>
      </template>
    </UCollapsible>
  </div>
</template>

<script setup lang="ts">
import type { SelectHaexIdentities } from '~/database/schemas'

export interface ListItemClaim {
  id: string
  type: string
  value: string
}

const emit = defineEmits<{
  toggle: [open: boolean]
  'share-qr': []
  'copy-did': []
  export: []
  edit: []
  delete: []
  'add-claim': []
  'copy-claim': [value: string]
  'edit-claim': [claim: ListItemClaim]
  'delete-claim': [claimId: string]
}>()

const { t } = useI18n()

const props = defineProps<{
  identity: SelectHaexIdentities
  expanded: boolean
  claims: ListItemClaim[]
}>()

const avatarOptions = computed(() => {
  if (!props.identity.avatarOptions) return null
  try {
    return JSON.parse(props.identity.avatarOptions) as Record<string, unknown>
  } catch {
    return null
  }
})

const menuItems = computed(() => [
  [
    {
      label: t('shareQr'),
      icon: 'i-lucide-qr-code',
      onSelect: () => emit('share-qr'),
    },
    {
      label: t('copyDid'),
      icon: 'i-lucide-copy',
      onSelect: () => emit('copy-did'),
    },
    {
      label: t('export'),
      icon: 'i-lucide-download',
      onSelect: () => emit('export'),
    },
    {
      label: t('edit'),
      icon: 'i-lucide-pencil',
      onSelect: () => emit('edit'),
    },
  ],
  [
    {
      label: t('delete'),
      icon: 'i-lucide-trash-2',
      color: 'error' as const,
      onSelect: () => emit('delete'),
    },
  ],
])
</script>

<i18n lang="yaml">
de:
  shareQr: QR-Code teilen
  copyDid: DID kopieren
  export: Exportieren
  edit: Bearbeiten
  delete: Löschen
  claimsTitle: Claims
  addClaim: Claim hinzufügen
  claimsEmpty: Keine Claims hinterlegt
en:
  shareQr: Share QR
  copyDid: Copy DID
  export: Export
  edit: Edit
  delete: Delete
  claimsTitle: Claims
  addClaim: Add claim
  claimsEmpty: No claims set
</i18n>
