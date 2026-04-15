import { and, eq, or, type SQLWrapper } from 'drizzle-orm'
import {
  haexNotifications,
  type InsertHaexNotifications,
} from '~/database/schemas'
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from '@tauri-apps/plugin-notification'
import { createLogger } from '@/stores/logging'
import { requireDb } from '~/stores/vault'

export interface IHaexNotification {
  id: string
  title: string | null
  text?: string | null
  icon?: string | null
  image?: string | null
  alt?: string | null
  date: string | null
  type?: 'error' | 'success' | 'warning' | 'info' | 'log' | null
}

const log = createLogger('NOTIFICATIONS')

export const useNotificationStore = defineStore('notificationStore', () => {
  const isNotificationAllowed = ref<boolean>(false)

  const requestNotificationPermissionAsync = async () => {
    const permission = await requestPermission()
    isNotificationAllowed.value = permission === 'granted'
  }

  const checkNotificationAsync = async () => {
    try {
      isNotificationAllowed.value = await isPermissionGranted()
    } catch (error) {
      log.warn('Notification permission check failed:', error)
      isNotificationAllowed.value = false
    }
    return isNotificationAllowed.value
  }

  const notifications = ref<IHaexNotification[]>([])

  const readNotificationsAsync = async (filter?: SQLWrapper[]) => {
    const db = requireDb()

    if (filter) {
      return await db
        .select()
        .from(haexNotifications)
        .where(and(...filter))
    } else {
      return await db.select().from(haexNotifications)
    }
  }

  const syncNotificationsAsync = async () => {
    notifications.value =
      (await readNotificationsAsync([eq(haexNotifications.read, false)])) ?? []
  }

  const addNotificationAsync = async (
    notification: Partial<InsertHaexNotifications>,
  ) => {
    try {
      const db = requireDb()
      const _notification: InsertHaexNotifications = {
        id: crypto.randomUUID(),
        alt: notification.alt,
        date: notification.date || new Date().toUTCString(),
        icon: notification.icon,
        image: notification.image,
        read: notification.read || false,
        source: notification.source,
        text: notification.text,
        title: notification.title,
        type: notification.type || 'info',
      }

      await db
        .insert(haexNotifications)
        .values(_notification)

      await syncNotificationsAsync()

      if (!isNotificationAllowed.value) {
        const permission = await requestPermission()
        isNotificationAllowed.value = permission === 'granted'
      }

      if (isNotificationAllowed.value) {
        sendNotification({
          title: _notification.title!,
          body: _notification.text!,
        })
      }
    } catch (error) {
      log.error('Failed to add notification:', error)
    }
  }

  const deleteNotificationsAsync = async (notificationIds: string[]) => {
    const db = requireDb()
    const filter = notificationIds.map((id) => eq(haexNotifications.id, id))

    return db
      .delete(haexNotifications)
      .where(or(...filter))
  }

  const reset = () => {
    notifications.value = []
  }

  return {
    addNotificationAsync,
    checkNotificationAsync,
    deleteNotificationsAsync,
    isNotificationAllowed,
    notifications,
    readNotificationsAsync,
    requestNotificationPermissionAsync,
    syncNotificationsAsync,
    reset,
  }
})
