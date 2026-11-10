<script setup lang="ts">
/**
 * The sidebar of the file browser (spec 044 FR-002): "Favoriten" with the known places of the device and
 * the folders the user added (spec 071), its drives
 * with their state and free space where known (spec 064: live, unmounted and locked ones too), the
 * other devices of the vault (spec 056 US1), the shares contacts
 * gave this vault (US2: expanding a contact asks its devices, FR-028), the list of the vault's
 * own shares (US4, FR-020), the sync rules of this device (spec 061), and the cloud storages of spec 038 (US5) with a button that leads to
 * adding one in the settings.
 */
import type { Drive } from '@bindings/Drive'
import type { FilesPeerState } from '@bindings/FilesPeerState'
import type { Sources } from '@bindings/Sources'
import type { FilesPin } from '~/lib/files/pins'
import { isCurrentDrive } from '~/lib/files/drives'
import type { StorageSpace } from '@bindings/StorageSpace'
import { formatFileSize } from '~/lib/passwords/format'

const props = defineProps<{
  sources: Sources | null
  /** The drives with their state (spec 064), kept live by the frame. */
  drives: readonly Drive[]
  /** Drives with an action running. */
  busy: ReadonlySet<string>
  /** The open folder of the device, `null` while a storage is open. */
  current: string | null
  /** The open storage. */
  currentStorage: string | null
  /** The open folder of the open place, for the added folders of other sources (spec 071). */
  currentPath: string | null
  /** The panel shows the sync rules (spec 061). */
  syncRulesCurrent?: boolean
  /** The panel shows the list of the vault's own shares (spec 071). */
  sharesCurrent?: boolean
}>()

const emit = defineEmits<{
  pick: [path: string]
  pickDrive: [drive: Drive]
  releaseDrive: [drive: Drive]
  pickStorage: [id: string]
  pickSyncRules: []
  pickShares: []
  pickPin: [pin: FilesPin]
  addStorage: []
  queryContact: [contactId: string]
}>()

const { t } = useI18n()
const { nameOf } = useFilesDriveActions()
const { storageSpaceAsync } = useFiles()
const { overview } = useStorageOverview()

/** What each cloud storage holds and may hold, where its provider tells (WebDAV, SFTP; not S3): read
 * again whenever the sidebar's sources are (a tab coming to the front). A storage that does not
 * answer shows nothing. */
const spaces = ref<Record<string, StorageSpace | null>>({})
let spacesRequest = 0

watch(
  () => props.sources?.storages,
  async (storages) => {
    const request = ++spacesRequest
    const next: Record<string, StorageSpace | null> = {}
    await Promise.all(
      (storages ?? []).map(async (storage) => {
        next[storage.id] = await storageSpaceAsync(storage.id).catch(() => null)
      }),
    )
    if (request === spacesRequest) spaces.value = next
  },
  { immediate: true },
)

function driveIcon(drive: Drive): string {
  if (props.busy.has(drive.id)) return 'lucide:loader-circle'
  if (drive.state === 'locked') return 'lucide:lock'
  return drive.kind === 'removable' ? 'lucide:usb' : 'lucide:hard-drive'
}

/** "Auswerfen" for a removable drive, "Aushängen" for an internal one (contracts/ui.md). */
function releaseTitle(drive: Drive): string {
  return t(
    drive.release === 'eject' ? 'files.drives.eject' : 'files.drives.unmount',
  )
}

function isCurrent(drive: Drive): boolean {
  return isCurrentDrive(drive, props.current, [...props.drives])
}

/** The protocol of a storage's connection (S3, WebDAV, SFTP), once the overview is read. */
function protocolOf(storageId: string): string | undefined {
  const storage = overview.value?.storages.find(
    (entry) => entry.id === storageId,
  )
  return overview.value?.connections.find(
    (connection) => connection.id === storage?.connectionId,
  )?.protocol
}

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
</script>

<template>
  <div class="flex h-full flex-col gap-4 overflow-y-auto p-2">
    <FilesPlaces
      :sources="sources"
      :current="current"
      :current-storage="currentStorage"
      :current-path="currentPath"
      @pick="emit('pick', $event)"
      @pick-pin="emit('pickPin', $event)"
    />
    <section v-if="drives.length">
      <h2 class="px-2 pb-1 text-xs font-medium text-muted-foreground">
        {{ t('files.sidebar.drives') }}
      </h2>
      <ul>
        <li v-for="drive in drives" :key="drive.id" class="flex items-center">
          <SidebarRow
            class="flex-1 disabled:cursor-wait disabled:opacity-100!"
            :class="drive.state === 'mounted' ? '' : 'text-muted-foreground'"
            :icon="driveIcon(drive)"
            :icon-class="
              busy.has(drive.id)
                ? 'animate-spin motion-reduce:animate-none'
                : ''
            "
            :active="isCurrent(drive)"
            :disabled="busy.has(drive.id)"
            :aria-busy="busy.has(drive.id)"
            :data-testid="`files-drive-${drive.id}-${drive.label ?? drive.id}`"
            @click="emit('pickDrive', drive)"
          >
            {{ nameOf(drive) }}
            <template #trailing>
              <span
                v-if="
                  drive.state === 'mounted' &&
                  drive.free !== null &&
                  drive.total === null
                "
                class="shrink-0 text-xs text-muted-foreground"
              >
                {{
                  t('files.sidebar.free', { size: formatFileSize(drive.free) })
                }}
              </span>
            </template>
          </SidebarRow>
          <FilesSpaceDonut
            v-if="
              drive.state === 'mounted' &&
              drive.total !== null &&
              drive.free !== null
            "
            :name="nameOf(drive)"
            :used="drive.total - drive.free"
            :total="drive.total"
            :data-testid="`files-drive-space-${drive.id}`"
          />
          <button
            v-if="drive.release !== 'none' && !busy.has(drive.id)"
            type="button"
            class="shrink-0 rounded p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
            :title="releaseTitle(drive)"
            :aria-label="`${releaseTitle(drive)}: ${nameOf(drive)}`"
            :data-testid="`files-drive-eject-${drive.id}-${drive.label ?? drive.id}`"
            @click="emit('releaseDrive', drive)"
          >
            <Icon name="lucide:arrow-up-from-line" class="size-4" />
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
          <SidebarRow
            :icon="PEER_ICONS[device.state]"
            :active="currentStorage === device.id"
            :disabled="device.state !== 'reachable'"
            :data-testid="`files-device-${device.id}`"
            @click="emit('pickStorage', device.id)"
          >
            {{ device.name ?? t('files.peer.unnamed') }}
            <template #trailing>
              <span
                v-if="device.state !== 'reachable'"
                class="shrink-0 text-xs text-muted-foreground"
              >
                {{ t(`files.peer.${device.state}`) }}
              </span>
            </template>
          </SidebarRow>
        </li>
      </ul>
    </section>
    <section v-if="sources?.contacts.length">
      <h2 class="px-2 pb-1 text-xs font-medium text-muted-foreground">
        {{ t('files.sidebar.contacts') }}
      </h2>
      <ul>
        <li v-for="contact in sources.contacts" :key="contact.contactId">
          <SidebarRow
            :icon="
              expanded.has(contact.contactId)
                ? 'lucide:chevron-down'
                : 'lucide:chevron-right'
            "
            :aria-expanded="expanded.has(contact.contactId)"
            :data-testid="`files-contact-${contact.contactName}`"
            @click="toggleContact(contact.contactId)"
          >
            {{ contact.contactName }}
          </SidebarRow>
          <ul v-if="expanded.has(contact.contactId)" class="pl-4">
            <li
              v-if="contact.shares.length === 0"
              class="px-2 py-1.5 text-xs text-muted-foreground"
            >
              {{ t('files.shares.noShares') }}
            </li>
            <li v-for="share in contact.shares" :key="share.storageId">
              <SidebarRow
                :class="share.reachable ? '' : 'opacity-60'"
                :icon="
                  share.itemKind === 'file'
                    ? 'lucide:file-symlink'
                    : 'lucide:folder-symlink'
                "
                :active="currentStorage === share.storageId"
                :data-testid="`files-share-${share.name}`"
                @click="emit('pickStorage', share.storageId)"
              >
                {{ share.name }}
                <template #trailing>
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
                </template>
              </SidebarRow>
            </li>
          </ul>
        </li>
      </ul>
    </section>
    <section v-if="sources">
      <SidebarRow
        icon="lucide:share-2"
        :active="sharesCurrent"
        data-testid="files-my-shares"
        @click="emit('pickShares')"
      >
        {{ t('files.sidebar.myShares') }}
      </SidebarRow>
      <SidebarRow
        icon="lucide:refresh-cw"
        :active="syncRulesCurrent"
        data-testid="files-sync-rules-open"
        @click="emit('pickSyncRules')"
      >
        {{ t('files.sidebar.syncRules') }}
      </SidebarRow>
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
        <li
          v-for="storage in sources.storages"
          :key="storage.id"
          class="flex items-center"
        >
          <SidebarRow
            class="flex-1"
            icon="lucide:cloud"
            :active="currentStorage === storage.id"
            :data-testid="`files-storage-${storage.name}`"
            @click="emit('pickStorage', storage.id)"
          >
            {{ storage.name }}
            <template #trailing>
              <span
                v-if="protocolOf(storage.id)"
                class="shrink-0 rounded border px-1 text-[10px] leading-4 font-medium text-muted-foreground"
                :data-testid="`files-storage-protocol-${storage.name}`"
              >
                {{ t(`settings.storage.protocols.${protocolOf(storage.id)}`) }}
              </span>
            </template>
          </SidebarRow>
          <FilesSpaceDonut
            v-if="spaces[storage.id]"
            :name="storage.name"
            :used="spaces[storage.id]!.used"
            :total="spaces[storage.id]!.total"
            :data-testid="`files-storage-space-${storage.name}`"
          />
        </li>
      </ul>
    </section>
  </div>
</template>
