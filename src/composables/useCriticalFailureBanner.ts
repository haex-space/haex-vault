import { invoke } from '@tauri-apps/api/core'

import type { CriticalNotification } from '~/../src-tauri/bindings/CriticalNotification'
import type { CriticalFailureCode } from '~/../src-tauri/bindings/CriticalFailureCode'
import type { Severity } from '~/../src-tauri/bindings/Severity'

/**
 * Reactive accessor + actions for the global critical-failure banner.
 *
 * Backend contract (Phase 2 PR A + B):
 *   - `crate::critical::lock_or_fail` writes one row to
 *     `haex_critical_notifications_no_sync` on every mutex-poison event.
 *   - UPSERT-deduped on `(code, location, acknowledged)`, so a flood of
 *     poison events collapses into a single banner row with a `count`.
 *   - The banner queries the NEWEST UNACKED row only — older unacked rows
 *     of OTHER `(code, location)` pairs still exist but stay hidden until
 *     the user acknowledges the visible one. This is intentional: the
 *     banner is a one-banner-at-a-time UI, the "+N more" affordance is a
 *     follow-up if real fleet data demands it.
 *
 * Polling cadence: 5 s. Cheap (one Tauri-IPC + one SQL SELECT against a
 * tiny table), and high enough to surface a poison within human reaction
 * time without busy-waiting. Polling is gated on vault-open state: when
 * no vault is mounted the backend returns `null` anyway, so the poll is
 * a no-op.
 */
export function useCriticalFailureBanner() {
  const { t, locale } = useI18n({
    useScope: 'global',
    messages: {
      de: {
        criticalFailures: {
          HlcMutexPoisoned: {
            title: 'Interner Uhrzeit-Dienst gestört',
            description:
              'Die HLC-Uhr des Vaults antwortet nicht mehr korrekt.',
            risk: 'Datenkorruption beim nächsten Sync ist möglich, wenn Du weiter arbeitest.',
            action:
              'Bitte starte den Vault neu. Deine Daten sind sicher auf der Festplatte gespeichert.',
            actionLabel: 'Vault neu starten',
          },
          DbMutexPoisoned: {
            title: 'Datenbank-Zugriff gestört',
            description:
              'Ein interner Lock auf die Datenbank ist beschädigt.',
            risk: 'Schreiboperationen schlagen ab jetzt fehl, bis der Vault neu gestartet wurde.',
            action: 'Bitte starte den Vault neu.',
            actionLabel: 'Vault neu starten',
          },
          DbSchemaDrift: {
            title: 'Datenbank-Schema unerwartet verändert',
            description:
              'Die laufende Version hat eine Tabelle oder Spalte vorgefunden, die sie so nicht lesen kann.',
            risk: 'Sync wird Daten möglicherweise nicht korrekt zusammenführen — bitte vor weiteren Schreiboperationen Backup prüfen.',
            action: 'Bitte starte den Vault neu und prüfe die System-Logs.',
            actionLabel: 'Vault neu starten',
          },
          AuditLogWriteFailed: {
            title: 'Audit-Log-Eintrag konnte nicht geschrieben werden',
            description:
              'Eine sicherheitsrelevante Aktion wurde nicht im Log gespeichert.',
            risk: 'Sicherheitsaudit unvollständig.',
            action:
              'Wir versuchen den Eintrag erneut zu schreiben — kein Neustart nötig, aber prüfe später die Logs.',
            actionLabel: 'Verstanden',
          },
          CrdtTransformFailed: {
            title: 'CRDT-Verarbeitung gestört',
            description:
              'Eine Datenbank-Anweisung konnte nicht für CRDT-Sync transformiert werden.',
            risk: 'Einzelne Abfragen liefern möglicherweise falsche Ergebnisse bis zum Neustart.',
            action: 'Wenn das Problem bestehen bleibt, starte den Vault neu.',
            actionLabel: 'Verstanden',
          },
          dismissed: 'Verstanden',
          unknownCode: 'Unbekannter Kritischer Fehler',
          countSuffix: '(×{count})',
        },
      },
      en: {
        criticalFailures: {
          HlcMutexPoisoned: {
            title: 'Internal clock service disrupted',
            description:
              "The vault's HLC clock stopped responding correctly.",
            risk:
              'Data corruption on next sync is possible if you continue using the vault.',
            action:
              'Please restart the vault. Your data is safely persisted to disk.',
            actionLabel: 'Restart vault',
          },
          DbMutexPoisoned: {
            title: 'Database access disrupted',
            description: 'An internal database lock is broken.',
            risk:
              'Write operations will fail until the vault has been restarted.',
            action: 'Please restart the vault.',
            actionLabel: 'Restart vault',
          },
          DbSchemaDrift: {
            title: 'Unexpected database schema state',
            description:
              'The running version found a table or column it cannot read.',
            risk:
              'Sync may not merge data correctly — please verify a backup before further writes.',
            action: 'Please restart the vault and check the system logs.',
            actionLabel: 'Restart vault',
          },
          AuditLogWriteFailed: {
            title: 'Audit log entry could not be written',
            description:
              'A security-relevant action was not recorded in the log.',
            risk: 'Security audit trail is incomplete.',
            action:
              'We will retry writing the entry — no restart needed, but please check the logs later.',
            actionLabel: 'Understood',
          },
          CrdtTransformFailed: {
            title: 'CRDT processing disrupted',
            description:
              'A database statement could not be transformed for CRDT sync.',
            risk:
              'Individual queries may return wrong results until restart.',
            action: 'If the problem persists, restart the vault.',
            actionLabel: 'Understood',
          },
          dismissed: 'Understood',
          unknownCode: 'Unknown critical failure',
          countSuffix: '(×{count})',
        },
      },
    },
  })

  /** Newest unacknowledged row, or `null` when nothing to show. */
  const current = ref<CriticalNotification | null>(null)
  /** True while the user's restart / acknowledge action is in flight. */
  const acting = ref(false)
  let pollHandle: ReturnType<typeof setInterval> | null = null

  /**
   * Map a backend `code` string to its `Severity`. Mirrors the Rust
   * `CriticalFailureCode::severity` mapping (see `src-tauri/src/critical/codes.rs`).
   *
   * If a new code is added on the Rust side without updating both this
   * map AND the i18n keys above, the banner falls back to `Critical`
   * severity and the `unknownCode` title — visible enough that the
   * missing translation gets caught in QA, not silent.
   */
  const severityForCode = (code: string): Severity => {
    switch (code as CriticalFailureCode) {
      case 'HlcMutexPoisoned':
      case 'DbMutexPoisoned':
      case 'DbSchemaDrift':
        return 'Critical'
      case 'AuditLogWriteFailed':
      case 'CrdtTransformFailed':
        return 'Warning'
      default:
        return 'Critical'
    }
  }

  /** Derived: the i18n strings for the current row's code. */
  const translatedContent = computed(() => {
    const row = current.value
    if (!row) return null
    const codeKey = row.code as CriticalFailureCode
    const base = `criticalFailures.${codeKey}` as const
    // Vue-i18n returns the key itself when no translation exists; we
    // detect that to fall back to a generic title rather than rendering
    // "criticalFailures.NewCode.title" verbatim.
    const titleRaw = t(`${base}.title`)
    const hasTranslation = titleRaw !== `${base}.title`
    return {
      severity: severityForCode(row.code),
      title: hasTranslation ? titleRaw : t('criticalFailures.unknownCode'),
      description: hasTranslation ? t(`${base}.description`) : row.code,
      risk: hasTranslation ? t(`${base}.risk`) : '',
      action: hasTranslation ? t(`${base}.action`) : '',
      actionLabel: hasTranslation
        ? t(`${base}.actionLabel`)
        : t('criticalFailures.dismissed'),
      // `count` from the backend is `bigint` (i64) — coerce to number for
      // display. Critical-notification counts realistically never exceed
      // 2^53 (would need a sustained poison-event-per-microsecond rate).
      countSuffix:
        row.count > 1
          ? t('criticalFailures.countSuffix', { count: Number(row.count) })
          : '',
    }
  })

  /** Pull the newest unacked row from the backend. Idempotent and cheap. */
  const refresh = async () => {
    try {
      const row = await invoke<CriticalNotification | null>(
        'critical_notifications_newest_unacked',
      )
      current.value = row
    } catch (err) {
      // Don't spam the user with their own banner system's failure —
      // log to console and try again on the next tick. Backend errors
      // here are unusual (sink mutex poisoned + main DB ok) and the
      // operator will already have a separate signal from stderr.
      console.warn('[CriticalBanner] poll failed:', err)
    }
  }

  /**
   * Acknowledge the currently shown row. After this returns, the next
   * `refresh()` either surfaces the next unacked row (if multiple
   * distinct (code, location) tuples have unacked rows) or hides the
   * banner.
   */
  const acknowledge = async () => {
    const row = current.value
    if (!row) return
    acting.value = true
    try {
      await invoke<number>('critical_notifications_acknowledge', { id: row.id })
      await refresh()
    } finally {
      acting.value = false
    }
  }

  /**
   * Restart the vault — Critical-severity codes recommend this; Warning
   * codes typically just need acknowledge. Tauri's app.restart() relaunches
   * the binary; the user's vault stays unlocked through the natural
   * unlock flow on next start.
   *
   * We also acknowledge the row first so it doesn't reappear on the next
   * launch (which would be misleading — the user already responded).
   */
  const restartApp = async () => {
    acting.value = true
    try {
      await invoke<number>('critical_notifications_acknowledge', {
        id: current.value?.id ?? '',
      })
      await invoke('critical_app_restart')
    } catch (err) {
      console.error('[CriticalBanner] restart failed:', err)
      acting.value = false
    }
  }

  // Polling lifecycle — start on mount, stop on unmount.
  const POLL_INTERVAL_MS = 5_000
  onMounted(() => {
    void refresh()
    pollHandle = setInterval(() => {
      void refresh()
    }, POLL_INTERVAL_MS)
  })
  onBeforeUnmount(() => {
    if (pollHandle !== null) {
      clearInterval(pollHandle)
      pollHandle = null
    }
  })

  return {
    current: readonly(current),
    translated: translatedContent,
    acting: readonly(acting),
    acknowledge,
    restartApp,
    /** Exposed for tests / explicit pull (e.g. after a known poison trigger). */
    refresh,
    /** Exposed for components that need to format severity differently. */
    severityForCode,
    /** Locale, in case a downstream consumer needs to react to language switch. */
    locale,
  }
}
