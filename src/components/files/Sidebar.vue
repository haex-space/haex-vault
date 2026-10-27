<script setup lang="ts">
/**
 * The sidebar of the file browser (spec 044 FR-002): the known places of the device, its drives
 * with free space where known, the other devices of the vault (spec 056 US1), the shares contacts
 * gave this vault (US2: expanding a contact asks its devices, FR-028), the overview of the vault's
 * own shares (US4, FR-020), the sync rules of this device (spec 061), and the cloud storages of spec 038 (US5) with a button that leads to
 * adding one in the settings.
 */
import type { FilesPeerState } from '@bindings/FilesPeerState'
import type { Sources } from '@bindings/Sources'
import { formatFileSize } from '~/lib/passwords/format'

defineProps<{
  sources: Sources | null
  /** The open folder of the device, `null` while a storage is open. */
  current: string | null
  /** The open storage. */
  currentStorage: string | null
}>()

const emit = defineEmits<{
  pick: [path: string]
  pickStorage: [id: string]
  addStorage: []
  queryContact: [contactId: string]
}>()

const { t } = useI18n()

/** "Meine Dateifreigaben" is open. */
const overviewOpen = ref(false)

/** "Sync-Regeln" is open (spec 061). */
const syncRulesOpen = ref(false)

/** The contacts expanded in this sidebar. */
const expanded = ref(new Set<string>())

function toggleContact(contactId: string) {
  const next = new Set(expanded.value)
  if (next.has(contactId)) next.delete(contactId)
  else {
    next.add(contactId)
    emit('queryContact', contactId)
  }
  expanded.value = next
}

/** A device that cannot be opened now stays in the list, greyed out (FR-011). */
const PEER_ICONS: Record<FilesPeerState, string> = {
  reachable: 'lucide:monitor-smartphone',
  unreachable: 'lucide:monitor-off',
  accessOff: 'lucide:lock',
}

const PLACE_ICONS: Record<string, string> = {
  home: 'lucide:house',
  pictures: 'lucide:image',
  downloads: 'lucide:download',
  documents: 'lucide:file-text',
  desktop: 'lucide:monitor',
  videos: 'lucide:film',
}
</script>

<template>
  <div class="flex h-full flex-col gap-4 overflow-y-auto p-2">
    <section v-if="sources?.known.length">
      <h2 class="px-2 pb-1 text-xs font-medium text-muted-foreground">
        {{ t('files.sidebar.places') }}
      </h2>
      <ul>
        <li v-for="place in sources.known" :key="place.path">
          <button
            type="button"
            class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm hover:bg-accent"
            :class="current === place.path ? 'bg-accent font-medium' : ''"
            :data-testid="`files-place-${place.name}`"
            @click="emit('pick', place.path)"
          >
            <Icon
              :name="PLACE_ICONS[place.name] ?? 'lucide:folder'"
              class="size-4 shrink-0"
            />
            <span class="truncate">{{ t(`files.places.${place.name}`) }}</span>
          </button>
        </li>
      </ul>
    </section>
    <section v-if="sources?.drives.length">
      <h2 class="px-2 pb-1 text-xs font-medium text-muted-foreground">
        {{ t('files.sidebar.drives') }}
      </h2>
      <ul>
        <li v-for="drive in sources.drives" :key="drive.path">
          <button
            type="button"
            class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm hover:bg-accent"
            :class="current === drive.path ? 'bg-accent font-medium' : ''"
            :data-testid="`files-drive-${drive.path}`"
            @click="emit('pick', drive.path)"
          >
            <Icon name="lucide:hard-drive" class="size-4 shrink-0" />
            <span class="min-w-0 flex-1 truncate">{{ drive.name }}</span>
            <span
              v-if="drive.free !== null"
              class="shrink-0 text-xs text-muted-foreground"
            >
              {{
                t('files.sidebar.free', { size: formatFileSize(drive.free) })
              }}
            </span>
          </button>
        </li>
      </ul>
    </section>
    <section v-if="sources?.devices.length">
      <h2 class="px-2 pb-1 text-xs font-medium text-muted-foreground">
        {{ t('files.sidebar.devices') }}
      </h2>
      <ul>
        <li v-for="device in sources.devices" :key="device.id">
          <button
            type="button"
            class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm enabled:hover:bg-accent disabled:cursor-not-allowed disabled:opacity-60"
            :class="currentStorage === device.id ? 'bg-accent font-medium' : ''"
            :disabled="device.state !== 'reachable'"
            :data-testid="`files-device-${device.id}`"
            @click="emit('pickStorage', device.id)"
          >
            <Icon :name="PEER_ICONS[device.state]" class="size-4 shrink-0" />
            <span class="min-w-0 flex-1 truncate">
              {{ device.name ?? t('files.peer.unnamed') }}
            </span>
            <span
              v-if="device.state !== 'reachable'"
              class="shrink-0 text-xs text-muted-foreground"
            >
              {{ t(`files.peer.${device.state}`) }}
            </span>
          </button>
        </li>
      </ul>
    </section>
    <section v-if="sources?.contacts.length">
      <h2 class="px-2 pb-1 text-xs font-medium text-muted-foreground">
        {{ t('files.sidebar.contacts') }}
      </h2>
      <ul>
        <li v-for="contact in sources.contacts" :key="contact.contactId">
          <button
            type="button"
            class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm hover:bg-accent"
            :aria-expanded="expanded.has(contact.contactId)"
            :data-testid="`files-contact-${contact.contactName}`"
            @click="toggleContact(contact.contactId)"
          >
            <Icon
              :name="
                expanded.has(contact.contactId)
                  ? 'lucide:chevron-down'
                  : 'lucide:chevron-right'
              "
              class="size-4 shrink-0"
            />
            <span class="truncate">{{ contact.contactName }}</span>
          </button>
          <ul v-if="expanded.has(contact.contactId)" class="pl-4">
            <li
              v-if="contact.shares.length === 0"
              class="px-2 py-1.5 text-xs text-muted-foreground"
            >
              {{ t('files.shares.noShares') }}
            </li>
            <li v-for="share in contact.shares" :key="share.storageId">
              <button
                type="button"
                class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm hover:bg-accent"
                :class="[
                  currentStorage === share.storageId
                    ? 'bg-accent font-medium'
                    : '',
                  share.reachable ? '' : 'opacity-60',
                ]"
                :data-testid="`files-share-${share.name}`"
                @click="emit('pickStorage', share.storageId)"
              >
                <Icon
                  :name="
                    share.itemKind === 'file'
                      ? 'lucide:file-symlink'
                      : 'lucide:folder-symlink'
                  "
                  class="size-4 shrink-0"
                />
                <span class="min-w-0 flex-1 truncate">{{ share.name }}</span>
                <span
                  v-if="!share.reachable || share.level === 'read'"
                  class="shrink-0 text-xs text-muted-foreground"
                >
                  {{
                    share.reachable
                      ? t('files.shares.readOnly')
                      : t('files.shares.unreachable')
                  }}
                </span>
              </button>
            </li>
          </ul>
        </li>
      </ul>
    </section>
    <section v-if="sources">
      <button
        type="button"
        class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm hover:bg-accent"
        data-testid="files-my-shares"
        @click="overviewOpen = true"
      >
        <Icon name="lucide:share-2" class="size-4 shrink-0" />
        <span class="truncate">{{ t('files.sidebar.myShares') }}</span>
      </button>
      <button
        type="button"
        class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm hover:bg-accent"
        data-testid="files-sync-rules-open"
        @click="syncRulesOpen = true"
      >
        <Icon name="lucide:refresh-cw" class="size-4 shrink-0" />
        <span class="truncate">{{ t('files.sidebar.syncRules') }}</span>
      </button>
    </section>
    <section v-if="sources">
      <div class="flex items-center justify-between gap-2 pb-1 pl-2">
        <h2 class="text-xs font-medium text-muted-foreground">
          {{ t('files.sidebar.storages') }}
        </h2>
        <UiButton
          variant="ghost"
          size="icon-sm"
          class="shrink-0"
          :aria-label="t('files.sidebar.addStorage')"
          :tooltip="t('files.sidebar.addStorage')"
          data-testid="files-add-storage"
          @click="emit('addStorage')"
        >
          <Icon name="lucide:plus" class="size-4" />
        </UiButton>
      </div>
      <ul>
        <li v-for="storage in sources.storages" :key="storage.id">
          <button
            type="button"
            class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm hover:bg-accent"
            :class="
              currentStorage === storage.id ? 'bg-accent font-medium' : ''
            "
            :data-testid="`files-storage-${storage.name}`"
            @click="emit('pickStorage', storage.id)"
          >
            <Icon name="lucide:cloud" class="size-4 shrink-0" />
            <span class="truncate">{{ storage.name }}</span>
          </button>
        </li>
      </ul>
    </section>
    <FilesSharesOverview
      v-model:open="overviewOpen"
      :contacts="sources?.contacts ?? []"
    />
    <FilesSyncRules v-model:open="syncRulesOpen" :sources="sources" />
  </div>
</template>
