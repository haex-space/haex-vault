import { listTime, type ListTimeLabels } from '~/lib/chat/listTime'

/**
 * Formats the times of the chat's list (spec 072 FR-001) in the interface language. The result
 * follows the clock, so "Jetzt" turns into "1 Min." without the list being reloaded.
 */
export function useListTime() {
  const { t, d } = useI18n()
  const now = useSharedNow()

  const labels = computed<ListTimeLabels>(() => ({
    now: t('chat.list.time.now'),
    minutes: (count) => t('chat.list.time.minutes', { count }),
    clock: (date) => d(date, { hour: '2-digit', minute: '2-digit' }),
    yesterday: t('chat.list.time.yesterday'),
    weekday: (date) => d(date, { weekday: 'short' }),
    date: (date, withYear) =>
      d(
        date,
        withYear
          ? { day: '2-digit', month: '2-digit', year: '2-digit' }
          : { day: '2-digit', month: '2-digit' },
      ),
  }))

  return (at: number) => listTime(at, labels.value, now.value)
}
