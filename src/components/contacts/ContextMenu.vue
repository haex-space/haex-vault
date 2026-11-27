<script setup lang="ts">
/**
 * The right-click menu of the address book (a long press on touch): around a contact it edits or
 * deletes it, around a group it renames or deletes the group, anywhere else it adds a contact or a
 * group. The innermost menu under the pointer opens. The own contact cannot be deleted.
 */
import type { ContactGroup } from '@bindings/ContactGroup'

const props = defineProps<{
  contact?: { id: string; name: string; isSelf?: boolean }
  group?: ContactGroup
}>()

const { t } = useI18n()
const actions = useContactsActions()
</script>

<template>
  <ShadcnContextMenu>
    <ShadcnContextMenuTrigger as-child>
      <slot />
    </ShadcnContextMenuTrigger>
    <ShadcnContextMenuContent
      class="min-w-48"
      data-testid="contacts-context-menu"
    >
      <template v-if="props.contact">
        <ShadcnContextMenuItem
          class="gap-2"
          data-testid="contacts-menu-edit"
          @select="actions.editContact(props.contact.id)"
        >
          <Icon name="lucide:pencil" class="size-4" />
          {{ t('contacts.edit') }}
        </ShadcnContextMenuItem>
        <ShadcnContextMenuItem
          v-if="!props.contact.isSelf"
          class="gap-2 text-destructive"
          data-testid="contacts-menu-delete"
          @select="actions.deleteContact(props.contact)"
        >
          <Icon name="lucide:trash-2" class="size-4" />
          {{ t('contacts.delete.title') }}
        </ShadcnContextMenuItem>
      </template>
      <template v-else-if="props.group">
        <ShadcnContextMenuItem
          class="gap-2"
          data-testid="contacts-menu-rename-group"
          @select="actions.renameGroup(props.group)"
        >
          <Icon name="lucide:pencil" class="size-4" />
          {{ t('contacts.groups.rename') }}
        </ShadcnContextMenuItem>
        <ShadcnContextMenuItem
          class="gap-2 text-destructive"
          data-testid="contacts-menu-delete-group"
          @select="actions.deleteGroup(props.group)"
        >
          <Icon name="lucide:trash-2" class="size-4" />
          {{ t('contacts.groups.delete') }}
        </ShadcnContextMenuItem>
      </template>
      <template v-else>
        <ShadcnContextMenuItem
          class="gap-2"
          data-testid="contacts-menu-new-contact"
          @select="actions.newContact()"
        >
          <Icon name="lucide:user-plus" class="size-4" />
          {{ t('contacts.new') }}
        </ShadcnContextMenuItem>
        <ShadcnContextMenuItem
          class="gap-2"
          data-testid="contacts-menu-new-group"
          @select="actions.newGroup()"
        >
          <Icon name="lucide:users" class="size-4" />
          {{ t('contacts.groups.new') }}
        </ShadcnContextMenuItem>
      </template>
    </ShadcnContextMenuContent>
  </ShadcnContextMenu>
</template>
