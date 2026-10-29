<script setup lang="ts">
/**
 * Kind, action and target of one extension permission, as the install dialog, the permission
 * question and the settings show it; an encrypted folder by its storage and name, a group of the
 * address book by its name, the destination of a model job by its provider, host and place. A wildcard (`*`) is spelled out and marked, so nobody mistakes
 * it for a narrow target: it allows everything of its kind.
 */
import {
  everythingKey,
  hasTarget,
  isEveryAction,
} from '~/lib/extensions/permissionScope'

import type { ExtensionModelDestination } from '@bindings/ExtensionModelDestination'
import type { FolderNames } from '@bindings/FolderNames'
import { placeLabelKey } from '~/lib/extensions/modelGrants'

const props = defineProps<{
  kind: string
  action: string
  target: string
  /** For an encrypted folder: its cloud storage and decrypted name (spec 048), shown instead of the
   * raw target. */
  folder?: FolderNames | null
  /** For a model job's destination: its provider, host and place (spec 055), instead of the
   * provider's id. */
  destination?: ExtensionModelDestination | null
  /** The kind is the heading of the list around it (the install dialog's groups): name only the
   * action. */
  hideKind?: boolean
}>()
const { t } = useI18n()

const everything = computed(() =>
  everythingKey(props.kind, props.action, props.target),
)
const everyAction = computed(() => isEveryAction(props.kind, props.action))
</script>

<template>
  <span class="flex flex-col">
    <span class="text-sm">
      <template v-if="!hideKind"
        >{{ t(`extensions.permissions.kinds.${kind}`, kind) }} ·
      </template>
      <span v-if="everyAction" class="font-medium text-warning">{{
        t('extensions.permissions.everyMethod')
      }}</span>
      <template v-else>{{ action }}</template>
    </span>
    <span
      v-if="everything"
      class="flex items-center gap-1 text-xs font-medium text-warning"
      data-testid="extension-permission-everything"
    >
      <Icon name="lucide:triangle-alert" class="size-3.5 shrink-0" />
      {{ t(`extensions.permissions.everything.${everything}`) }}
    </span>
    <span
      v-else-if="folder"
      class="text-xs break-all"
      data-testid="extension-permission-folder"
    >
      {{
        t(
          folder.onDevice
            ? 'extensions.permissionRequest.encryptedFolderDevice'
            : 'extensions.permissionRequest.encryptedFolder',
          { folder: folder.folderName, storage: folder.storageName },
        )
      }}
    </span>
    <span
      v-else-if="destination"
      class="text-xs break-all"
      data-testid="extension-permission-destination"
    >
      {{ destination.provider }} ·
      <template v-if="destination.host">{{ destination.host }} · </template>
      {{ t(placeLabelKey(destination.place)) }}
    </span>
    <span
      v-else-if="kind === 'peers'"
      class="text-xs"
      data-testid="extension-permission-peers"
    >
      {{ t('extensions.permissions.peers.description') }}
    </span>
    <span
      v-else-if="kind === 'contacts'"
      class="text-xs break-all"
      data-testid="extension-permission-contacts-group"
    >
      {{ t('extensions.permissions.contactsGroup', { name: target }) }}
    </span>
    <span v-else-if="hasTarget(kind)" class="font-mono text-xs break-all">{{
      target
    }}</span>
  </span>
</template>
