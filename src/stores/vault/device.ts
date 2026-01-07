import { load } from '@tauri-apps/plugin-store'
import {
  hostname as tauriHostname,
  platform as tauriPlatform,
} from '@tauri-apps/plugin-os'
import { eq } from 'drizzle-orm'
import { haexDevices } from '~/database/schemas'

const deviceIdKey = 'deviceId'
const defaultDeviceFileName = 'device.json'

export const useDeviceStore = defineStore('vaultDeviceStore', () => {
  const deviceId = ref<string | undefined>('')

  const syncDeviceIdAsync = async () => {
    deviceId.value = await getDeviceIdAsync()
    if (deviceId.value) return deviceId.value

    deviceId.value = await setDeviceIdAsync()
  }

  const platform = computedAsync(() => tauriPlatform())

  /** True for mobile platforms (iOS, Android) */
  const isMobile = computed(() => {
    const p = platform.value
    return p === 'ios' || p === 'android'
  })

  /** True for desktop platforms (everything except iOS/Android) */
  const isDesktop = computed(() => !isMobile.value)

  const hostname = computedAsync(() => tauriHostname())

  const deviceName = ref<string>()

  const getDeviceIdAsync = async () => {
    const store = await getStoreAsync()
    return await store.get<string>(deviceIdKey)
  }

  const getStoreAsync = async () => {
    const {
      public: { haexVault },
    } = useRuntimeConfig()

    return await load(haexVault.deviceFileName || defaultDeviceFileName)
  }

  const setDeviceIdAsync = async (id?: string) => {
    const store = await getStoreAsync()
    const _id = id || crypto.randomUUID()
    await store.set(deviceIdKey, _id)
    return _id
  }

  const isKnownDeviceAsync = async () => {
    const device = await readDeviceAsync(deviceId.value)
    console.log('device', device)
    return !!device
  }

  const readDeviceAsync = async (id?: string) => {
    const { currentVault } = useVaultStore()

    if (!id) return undefined

    const device = await currentVault?.drizzle?.query.haexDevices.findFirst({
      where: eq(haexDevices.deviceId, id),
    })

    // Workaround für Drizzle Bug: findFirst gibt manchmal Objekt mit undefined Werten zurück
    // https://github.com/drizzle-team/drizzle-orm/issues/3872
    // Prüfe ob das Device wirklich existiert (id muss gesetzt sein, da NOT NULL)
    if (!device?.id) return undefined

    return device
  }

  const readDeviceNameAsync = async (id?: string) => {
    const _id = id || deviceId.value

    if (!_id) return

    const device = await readDeviceAsync(_id)
    deviceName.value = device?.name ?? ''

    return deviceName.value
  }

  const updateDeviceNameAsync = async ({
    id,
    name,
  }: {
    id?: string
    name?: string
  }) => {
    const { currentVault } = useVaultStore()
    const _id = id ?? deviceId.value
    if (!_id || !name) return

    deviceName.value = name

    return currentVault?.drizzle
      ?.update(haexDevices)
      .set({
        name,
      })
      .where(eq(haexDevices.deviceId, _id))
  }

  const addDeviceNameAsync = async ({
    id,
    name,
  }: {
    id?: string
    name: string
  }) => {
    const { currentVault } = useVaultStore()
    const _id = id ?? deviceId.value
    if (!_id || !name) throw new Error('Id oder Name fehlen')

    return currentVault?.drizzle?.insert(haexDevices).values({
      deviceId: _id,
      name,
    })
  }

  /**
   * Sets the current device in the vault's haex_devices table
   * Marks all other devices as not current and the given device as current
   */
  const setAsCurrentDeviceAsync = async (id?: string) => {
    const { currentVault } = useVaultStore()

    if (!currentVault?.drizzle) {
      throw new Error('No vault opened')
    }

    const _deviceId = id ?? deviceId.value
    if (!_deviceId) {
      throw new Error('Device ID not available')
    }

    // First, set all devices to current = false
    await currentVault.drizzle
      .update(haexDevices)
      .set({ current: false })

    // Check if device exists
    const existingDevice = await readDeviceAsync(_deviceId)

    if (existingDevice) {
      // Device exists, just update current flag
      await currentVault.drizzle
        .update(haexDevices)
        .set({ current: true })
        .where(eq(haexDevices.deviceId, _deviceId))
      console.log(`✅ Set existing device as current: ${_deviceId}`)
    } else {
      console.log(`⚠️  Device does not exist yet, cannot set as current: ${_deviceId}`)
    }
  }

  return {
    addDeviceNameAsync,
    deviceId,
    deviceName,
    getDeviceIdAsync,
    hostname,
    isDesktop,
    isKnownDeviceAsync,
    isMobile,
    platform,
    readDeviceAsync,
    readDeviceNameAsync,
    setAsCurrentDeviceAsync,
    setDeviceIdAsync,
    syncDeviceIdAsync,
    updateDeviceNameAsync,
  }
})
