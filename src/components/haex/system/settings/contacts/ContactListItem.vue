<template>
  <div class="p-3 rounded-lg border border-default">
    <UCollapsible
      :open="expanded"
      :unmount-on-hide="false"
      @update:open="(val: boolean) => $emit('toggle', contact.id, val)"
    >
      <div class="flex items-center justify-between cursor-pointer">
        <div class="flex-1 min-w-0 flex items-center gap-2">
          <UIcon
            name="i-lucide-chevron-right"
            class="w-4 h-4 shrink-0 text-muted transition-transform duration-200"
            :class="{ 'rotate-90': expanded }"
          />
          <UiAvatar
            :src="contact.avatar"
            :seed="contact.id"
            size="sm"
          />
          <div class="min-w-0">
            <div class="flex items-center gap-2">
              <span class="font-medium truncate">{{ contact.label }}</span>
            </div>
            <div class="mt-1 flex items-center gap-2">
              <code class="text-xs text-muted truncate max-w-[300px]">{{
                contact.publicKey
              }}</code>
            </div>
          </div>
        </div>

        <div
          class="shrink-0 ml-4"
          @click.stop
        >
          <!-- Large screens: inline buttons -->
          <div class="hidden @md:flex items-center gap-1">
            <UButton
              variant="ghost"
              icon="i-lucide-copy"
              :title="t('actions.copyKey')"
              @click="copyPublicKey"
            />
            <UButton
              variant="ghost"
              icon="i-lucide-pencil"
              :title="t('actions.edit')"
              @click="$emit('edit', contact)"
            />
            <UButton
              variant="ghost"
              color="error"
              icon="i-lucide-trash-2"
              :title="t('actions.delete')"
              @click="$emit('delete', contact)"
            />
          </div>
          <!-- Small screens: dropdown menu -->
          <UDropdownMenu
            class="@md:hidden"
            :items="[
              [
                {
                  label: t('actions.copyKey'),
                  icon: 'i-lucide-copy',
                  onSelect: () => copyPublicKey(),
                },
                {
                  label: t('actions.edit'),
                  icon: 'i-lucide-pencil',
                  onSelect: () => $emit('edit', contact),
                },
              ],
              [
                {
                  label: t('actions.delete'),
                  icon: 'i-lucide-trash-2',
                  color: 'error' as const,
                  onSelect: () => $emit('delete', contact),
                },
              ],
            ]"
          >
            <UButton
              variant="ghost"
              icon="i-lucide-ellipsis-vertical"
              color="neutral"
            />
          </UDropdownMenu>
        </div>
      </div>

      <!-- Claims Section (collapsible) -->
      <template
        v-if="expanded"
        #content
      >
        <div class="mt-3 pt-3 border-t border-default space-y-2">
          <div class="flex items-center justify-between">
            <span class="text-sm font-medium">{{ t('claims.title') }}</span>
            <UButton
              variant="outline"
              icon="i-lucide-plus"
              @click="$emit('addClaim', contact.id)"
            >
              {{ t('claims.add') }}
            </UButton>
          </div>

          <div
            v-if="claims.length"
            class="space-y-1"
          >
            <div
              v-for="claim in claims"
              :key="claim.id"
              class="flex items-center justify-between p-2 rounded bg-gray-50 dark:bg-gray-800/50"
            >
              <div class="min-w-0 flex-1">
                <span class="text-xs font-medium text-muted">{{
                  claim.type
                }}</span>
                <p class="text-sm truncate">{{ claim.value }}</p>
              </div>
              <div class="flex gap-1 shrink-0 ml-2">
                <UButton
                  variant="ghost"
                  icon="i-lucide-copy"
                  @click="copyClaimValue(claim.value)"
                />
                <UButton
                  variant="ghost"
                  icon="i-lucide-pencil"
                  @click="$emit('editClaim', claim)"
                />
                <UButton
                  variant="ghost"
                  color="error"
                  icon="i-lucide-trash-2"
                  @click="$emit('deleteClaim', claim.id, contact.id)"
                />
              </div>
            </div>
          </div>
          <p
            v-else
            class="text-xs text-muted"
          >
            {{ t('claims.empty') }}
          </p>

          <!-- Notes -->
          <div
            v-if="contact.notes"
            class="pt-2"
          >
            <span class="text-xs font-medium text-muted">{{
              t('fields.notes')
            }}</span>
            <p class="text-sm text-muted">{{ contact.notes }}</p>
          </div>
        </div>
      </template>
    </UCollapsible>
  </div>
</template>

<script setup lang="ts">
import type { SelectHaexIdentities } from '~/database/schemas'

const props = defineProps<{
  contact: SelectHaexIdentities
  expanded: boolean
  claims: { id: string; type: string; value: string }[]
}>()

defineEmits<{
  toggle: [contactId: string, open: boolean]
  edit: [contact: SelectHaexIdentities]
  delete: [contact: SelectHaexIdentities]
  addClaim: [contactId: string]
  editClaim: [claim: { id: string; type: string; value: string }]
  deleteClaim: [claimId: string, contactId: string]
}>()

const { t } = useI18n()
const { add: addToast } = useToast()

const copyPublicKey = async () => {
  try {
    await navigator.clipboard.writeText(props.contact.publicKey)
    addToast({ title: t('success.copied'), color: 'success' })
  } catch {
    addToast({ title: t('errors.copyFailed'), color: 'error' })
  }
}

const copyClaimValue = async (value: string) => {
  try {
    await navigator.clipboard.writeText(value)
    addToast({ title: t('success.copied'), color: 'success' })
  } catch {
    addToast({ title: t('errors.copyFailed'), color: 'error' })
  }
}
</script>

<i18n lang="yaml">
de:
  fields:
    notes: Notizen
  claims:
    title: Claims
    add: Hinzufügen
    empty: Keine Claims vorhanden.
  actions:
    copyKey: Public Key kopieren
    edit: Bearbeiten
    delete: Löschen
  success:
    copied: Kopiert
  errors:
    copyFailed: Kopieren fehlgeschlagen
en:
  fields:
    notes: Notes
  claims:
    title: Claims
    add: Add
    empty: No claims yet.
  actions:
    copyKey: Copy public key
    edit: Edit
    delete: Delete
  success:
    copied: Copied
  errors:
    copyFailed: Failed to copy
</i18n>
