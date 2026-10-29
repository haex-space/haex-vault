<script setup lang="ts">
/**
 * The sidebar of the file browser (spec 044 FR-002): the known places of the device, its drives
 * with their state and free space where known (spec 064: live, unmounted and locked ones too), the
 * other devices of the vault (spec 056 US1), the shares contacts
 * gave this vault (US2: expanding a contact asks its devices, FR-028), the overview of the vault's
 * own shares (US4, FR-020), the sync rules of this device (spec 061), and the cloud storages of spec 038 (US5) with a button that leads to
 * adding one in the settings.
 */
import type { Drive } from '@bindings/Drive'
import type { FilesPeerState } from '@bindings/FilesPeerState'
import type { Sources } from '@bindings/Sources'
import { isCurrentDrive } from '~/lib/files/drives'
import type { StorageSpace } from '@bindings/StorageSpace'
import { formatSpace } from '~/lib/files/space'
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
  /** The panel shows the sync rules (spec 061). */
  syncRulesCurrent?: boolean
}>()

const emit = defineEmits<{
  pick: [path: string]
  pickDrive: [drive: Drive]
  releaseDrive: [drive: Drive]
  pickStorage: [id: string]
  pickSyncRules: []
  addStorage: []
  queryContact: [contactId: string]
}>()

const { t, locale } = useI18n()
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

/** `12,3/100 GB` of a storage, or `undefined` while nothing is known. */
function storageSpaceText(id: string): string | undefined {
  const space = spaces.value[id]
  return space ? formatSpace(space.used, space.total, locale.value) : undefined
}

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

/** "Meine Dateifreigaben" is open. */
const overviewOpen = ref(false)

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
    <section v-if="drives.length">
      <h2 class="px-2 pb-1 text-xs font-medium text-muted-foreground">
        {{ t('files.sidebar.drives') }}
      </h2>
      <ul>
        <li v-for="drive in drives" :key="drive.id" class="flex items-center">
          <button
            type="button"
            class="flex min-w-0 flex-1 items-center gap-2 rounded px-2 py-1.5 text-left text-sm enabled:hover:bg-accent disabled:cursor-wait"
            :class="[
              isCurrent(drive) ? 'bg-accent font-medium' : '',
              drive.state === 'mounted' ? '' : 'text-muted-foreground',
            ]"
            :disabled="busy.has(drive.id)"
            :aria-busy="busy.has(drive.id)"
            :data-testid="`files-drive-${drive.id}-${drive.label ?? drive.id}`"
            @click="emit('pickDrive', drive)"
          >
            <Icon
              :name="driveIcon(drive)"
              :class="[
                'size-4 shrink-0',
                busy.has(drive.id) && 'animate-spin motion-reduce:animate-none',
              ]"
            />
            <span class="min-w-0 flex-1 truncate">{{ nameOf(drive) }}</span>
            <span
              v-if="
                drive.state === 'mounted' &&
                drive.total !== null &&
                drive.free !== null
              "
              class="shrink-0 text-xs text-muted-foreground"
              :title="
                t('files.sidebar.freeOf', {
                  free: formatSpace(drive.free, null, locale),
                  total: formatSpace(drive.total, null, locale),
                })
              "
              :data-testid="`files-drive-space-${drive.id}`"
            >
              {{ formatSpace(drive.total - drive.free, drive.total, locale) }}
            </span>
            <span
              v-else-if="drive.state === 'mounted' && drive.free !== null"
              class="shrink-0 text-xs text-muted-foreground"
            >
              {{
                t('files.sidebar.free', { size: formatFileSize(drive.free) })
              }}
            </span>
          </button>
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
        :class="syncRulesCurrent ? 'bg-accent font-medium' : ''"
        @click="emit('pickSyncRules')"
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
            <span class="min-w-0 flex-1">
              <span class="block truncate">{{ storage.name }}</span>
              <span
                v-if="storageSpaceText(storage.id)"
                class="block truncate text-xs text-muted-foreground"
                :data-testid="`files-storage-space-${storage.name}`"
              >
                {{ storageSpaceText(storage.id) }}
              </span>
            </span>
            <span
              v-if="protocolOf(storage.id)"
              class="shrink-0 rounded border px-1 text-[10px] leading-4 font-medium text-muted-foreground"
              :data-testid="`files-storage-protocol-${storage.name}`"
            >
              {{ t(`settings.storage.protocols.${protocolOf(storage.id)}`) }}
            </span>
          </button>
        </li>
      </ul>
    </section>
    <FilesSharesOverview
      v-model:open="overviewOpen"
      :contacts="sources?.contacts ?? []"
    />
  </div>
</template>
