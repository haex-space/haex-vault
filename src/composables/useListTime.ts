import { listTime, type ListTimeLabels } from '~/lib/chat/listTime'

/** How often the relative times of the list ("48 Min.") are looked at again. */
const REFRESH_MS = 30_000

// One clock for every entry of the list: it runs while at least one entry is on screen.
const now = shallowRef(new Date())
let watchers = 0
let timer: ReturnType<typeof setInterval> | null = null

function followClock() {
  if (watchers++ === 0) {
    now.value = new Date()
    timer = setInterval(() => {
      now.value = new Date()
    }, REFRESH_MS)
  }
  onScopeDispose(() => {
    if (--watchers === 0 && timer !== null) {
      clearInterval(timer)
      timer = null
    }
  })
}

/**
 * Formats the times of the chat's list (spec 072 FR-001) in the interface language. The result
 * follows the clock, so "Jetzt" turns into "1 Min." without the list being reloaded.
 */
export function useListTime() {
  const { t, d } = useI18n()
  followClock()

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
