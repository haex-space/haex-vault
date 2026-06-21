<template>
  <HaexSystemSettingsLayout :title="t('title')" :description="t('description')">
    <UCard>
      <template #header>
        <h3 class="text-lg font-semibold">{{ t('thresholds.title') }}</h3>
        <p class="text-sm text-muted">{{ t('thresholds.description') }}</p>
      </template>

      <form class="space-y-6" @submit.prevent="onSaveAsync">
        <section class="space-y-4">
          <h4 class="font-medium">{{ t('l4.title') }}</h4>
          <UFormField :label="t('l4.warn.label')" :description="t('l4.warn.description')">
            <UiInput v-model.number="form.l4WarnPerSec" type="number" min="1" />
          </UFormField>
          <UFormField :label="t('l4.sample.label')" :description="t('l4.sample.description')">
            <UiInput v-model.number="form.l4SamplePerSec" type="number" min="1" />
          </UFormField>
        </section>

        <section class="space-y-4">
          <h4 class="font-medium">{{ t('l1.title') }}</h4>
          <UFormField :label="t('l1.global.label')" :description="t('l1.global.description')">
            <UiInput v-model.number="form.l1GlobalPerSec" type="number" min="1" />
          </UFormField>
          <UFormField :label="t('l1.perSource.label')" :description="t('l1.perSource.description')">
            <UiInput v-model.number="form.l1PerSourcePerSec" type="number" min="1" />
          </UFormField>
          <UFormField :label="t('l2.label')" :description="t('l2.description')">
            <UiInput v-model.number="form.l2MaxStreams" type="number" min="1" />
          </UFormField>
          <UFormField :label="t('l3.label')" :description="t('l3.description')">
            <UiInput v-model.number="form.l3TimeoutSecs" type="number" min="1" />
          </UFormField>
        </section>

        <section class="space-y-4">
          <h4 class="font-medium">{{ t('ddos.title') }}</h4>
          <UFormField :label="t('ddos.distinctSources.label')" :description="t('ddos.distinctSources.description')">
            <UiInput v-model.number="form.ddosDistinctSources" type="number" min="1" />
          </UFormField>
          <UFormField :label="t('ddos.escalation.label')" :description="t('ddos.escalation.description')">
            <USelect
              v-model="form.ddosEscalation"
              :items="escalationItems"
              value-key="value"
            />
          </UFormField>
          <UFormField :label="t('ddos.expiry.label')" :description="t('ddos.expiry.description')">
            <UiInput v-model.number="form.ddosAutoExpirySecs" type="number" min="60" />
          </UFormField>
        </section>
      </form>

      <template #footer>
        <div class="flex justify-between">
          <UiButton
            color="neutral"
            variant="outline"
            :disabled="saving"
            @click="onResetDefaults"
          >
            {{ t('actions.resetDefaults') }}
          </UiButton>
          <UiButton :loading="saving" :disabled="!dirty" @click="onSaveAsync">
            {{ t('actions.save') }}
          </UiButton>
        </div>
      </template>
    </UCard>
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { eq } from 'drizzle-orm'
import * as schema from '~/database/schemas'
import { requireDb } from '~/stores/vault'

const { t } = useI18n()
const { add } = useToast()

// Keys must mirror src-tauri/.../dos_defence/config.rs KEY_* constants
const KEYS = {
  l1GlobalPerSec: 'dosDefence.l1.globalRatePerSec',
  l1PerSourcePerSec: 'dosDefence.l1.perSourceRatePerSec',
  l2MaxStreams: 'dosDefence.l2.maxStreamsPerConn',
  l3TimeoutSecs: 'dosDefence.l3.handshakeTimeoutSecs',
  l4WarnPerSec: 'dosDefence.l4.rejectRateThresholdPerSec',
  l4SamplePerSec: 'dosDefence.l4.sampleThresholdPerSec',
  ddosDistinctSources: 'dosDefence.ddos.distinctSourcesThreshold',
  ddosEscalation: 'dosDefence.ddos.escalationPolicy',
  ddosAutoExpirySecs: 'dosDefence.ddos.autoExpirySecs',
} as const

const DEFAULTS = {
  l1GlobalPerSec: 100,
  l1PerSourcePerSec: 10,
  l2MaxStreams: 8,
  l3TimeoutSecs: 5,
  l4WarnPerSec: 20,
  l4SamplePerSec: 100,
  ddosDistinctSources: 10,
  ddosEscalation: 'contacts_only' as 'contacts_only' | 'off',
  ddosAutoExpirySecs: 1800,
}

const form = reactive({ ...DEFAULTS })
const original = reactive({ ...DEFAULTS })
const saving = ref(false)

const escalationItems = computed(() => [
  { label: t('ddos.escalation.contactsOnly'), value: 'contacts_only' },
  { label: t('ddos.escalation.off'), value: 'off' },
])

const dirty = computed(() =>
  (Object.keys(form) as Array<keyof typeof form>).some(
    (k) => form[k] !== original[k],
  ),
)

/**
 * Coerce any user-entered or DB-stored value to a positive integer at or
 * above `min`. The Rust-side parser is strict: `"20.5"`, `"-5"`, or
 * `"abc"` all fail `.parse::<u32>()` and silently fall back to defaults
 * on the next leader restart. Without the same coercion here the UI
 * would show a value the backend never actually applies. See CodeRabbit
 * review on PR #491.
 */
const clampToInt = (value: unknown, min: number, fallback: number): number => {
  const n = Number(value)
  if (!Number.isFinite(n)) return fallback
  const i = Math.floor(n)
  return i >= min ? i : fallback
}

const loadAsync = async () => {
  const db = requireDb()
  const rows = await db.query.haexVaultSettings.findMany()
  const map = new Map<string, string>()
  for (const r of rows) if (r.value) map.set(r.key, r.value)

  form.l1GlobalPerSec = clampToInt(map.get(KEYS.l1GlobalPerSec), 1, DEFAULTS.l1GlobalPerSec)
  form.l1PerSourcePerSec = clampToInt(map.get(KEYS.l1PerSourcePerSec), 1, DEFAULTS.l1PerSourcePerSec)
  form.l2MaxStreams = clampToInt(map.get(KEYS.l2MaxStreams), 1, DEFAULTS.l2MaxStreams)
  form.l3TimeoutSecs = clampToInt(map.get(KEYS.l3TimeoutSecs), 1, DEFAULTS.l3TimeoutSecs)
  form.l4WarnPerSec = clampToInt(map.get(KEYS.l4WarnPerSec), 1, DEFAULTS.l4WarnPerSec)
  form.l4SamplePerSec = clampToInt(map.get(KEYS.l4SamplePerSec), 1, DEFAULTS.l4SamplePerSec)
  form.ddosDistinctSources = clampToInt(map.get(KEYS.ddosDistinctSources), 1, DEFAULTS.ddosDistinctSources)
  form.ddosAutoExpirySecs = clampToInt(map.get(KEYS.ddosAutoExpirySecs), 60, DEFAULTS.ddosAutoExpirySecs)
  const esc = map.get(KEYS.ddosEscalation)
  form.ddosEscalation = esc === 'off' ? 'off' : 'contacts_only'

  Object.assign(original, form)
}

const validateForm = () => {
  form.l1GlobalPerSec = clampToInt(form.l1GlobalPerSec, 1, DEFAULTS.l1GlobalPerSec)
  form.l1PerSourcePerSec = clampToInt(form.l1PerSourcePerSec, 1, DEFAULTS.l1PerSourcePerSec)
  form.l2MaxStreams = clampToInt(form.l2MaxStreams, 1, DEFAULTS.l2MaxStreams)
  form.l3TimeoutSecs = clampToInt(form.l3TimeoutSecs, 1, DEFAULTS.l3TimeoutSecs)
  form.l4WarnPerSec = clampToInt(form.l4WarnPerSec, 1, DEFAULTS.l4WarnPerSec)
  form.l4SamplePerSec = clampToInt(form.l4SamplePerSec, 1, DEFAULTS.l4SamplePerSec)
  form.ddosDistinctSources = clampToInt(form.ddosDistinctSources, 1, DEFAULTS.ddosDistinctSources)
  form.ddosAutoExpirySecs = clampToInt(form.ddosAutoExpirySecs, 60, DEFAULTS.ddosAutoExpirySecs)
}

const onSaveAsync = async () => {
  saving.value = true
  try {
    validateForm()

    const writes: Array<[string, string]> = [
      [KEYS.l1GlobalPerSec, String(form.l1GlobalPerSec)],
      [KEYS.l1PerSourcePerSec, String(form.l1PerSourcePerSec)],
      [KEYS.l2MaxStreams, String(form.l2MaxStreams)],
      [KEYS.l3TimeoutSecs, String(form.l3TimeoutSecs)],
      [KEYS.l4WarnPerSec, String(form.l4WarnPerSec)],
      [KEYS.l4SamplePerSec, String(form.l4SamplePerSec)],
      [KEYS.ddosDistinctSources, String(form.ddosDistinctSources)],
      [KEYS.ddosEscalation, form.ddosEscalation],
      [KEYS.ddosAutoExpirySecs, String(form.ddosAutoExpirySecs)],
    ]

    // Atomic batch: a partial write would leave the DoS policy in a
    // mixed state at next leader restart. Wrap all nine upserts in a
    // single transaction so either all land or none do. See CodeRabbit
    // review on PR #491.
    const db = requireDb()
    await db.transaction(async (tx) => {
      for (const [key, value] of writes) {
        const existing = await tx.query.haexVaultSettings.findFirst({
          where: eq(schema.haexVaultSettings.key, key),
        })
        if (existing) {
          await tx
            .update(schema.haexVaultSettings)
            .set({ value })
            .where(eq(schema.haexVaultSettings.key, key))
        } else {
          await tx.insert(schema.haexVaultSettings).values({
            id: crypto.randomUUID(),
            key,
            value,
          })
        }
      }
    })
    Object.assign(original, form)
    add({
      title: t('success.saved'),
      description: t('success.savedHint'),
      color: 'success',
    })
  } catch (e) {
    add({
      title: t('errors.saveFailed'),
      description: e instanceof Error ? e.message : String(e),
      color: 'error',
    })
  } finally {
    saving.value = false
  }
}

const onResetDefaults = () => {
  Object.assign(form, DEFAULTS)
}

onMounted(loadAsync)
</script>

<i18n lang="yaml">
de:
  title: Sicherheit & Schutz
  description: Schwellwerte für die DoS-Abwehr. Änderungen gelten ab dem nächsten Leader-Neustart.
  thresholds:
    title: Erkennungs-Schwellwerte
    description: Werte sind pro Sekunde im gleitenden 1-Sekunden-Fenster. Niedrigere Werte sind empfindlicher (mehr falsch-positive Banner), höhere Werte toleranter.
  l4:
    title: AuthGate-Rate-Limit
    warn:
      label: Warn-Schwelle (Rejects/sec pro Peer)
      description: Über diesem Wert erhältst Du eine Banner-Warnung für die betroffene Peer-DID. Default 20.
    sample:
      label: Sampling-Schwelle (Rejects/sec pro Peer)
      description: Ab diesem Wert wird nur noch ein Bruchteil der Reject-Logs geschrieben, um die Datenbank nicht zu fluten. Default 100.
  l1:
    title: Verbindungs-Limits (Phase 2)
    global:
      label: Globale Verbindungen pro Sekunde
      description: Gesamt-Limit für eingehende QUIC-Connects. Default 100.
    perSource:
      label: Verbindungen pro Quell-Endpunkt
      description: Limit pro Quell-endpoint_id. Default 10. Wird erst in Phase 2 enforced.
  l2:
    label: Streams pro Verbindung
    description: Maximal gleichzeitig offene Streams. Default 8.
  l3:
    label: Handshake-Timeout (Sekunden)
    description: Wie lange darf der Stream offen bleiben ohne Handshake-Abschluss. Default 5.
  ddos:
    title: DDoS-Eskalation (Phase 3)
    distinctSources:
      label: Distinct-Source-Schwelle
      description: Bei wie vielen unterschiedlichen Quellen gleichzeitig flooden wir auf den DDoS-Modus eskalieren. Default 10.
    escalation:
      label: Eskalations-Verhalten
      description: Was passiert wenn ein DDoS erkannt wird.
      contactsOnly: Nur Kontakte akzeptieren
      off: Aus (nur Notification)
    expiry:
      label: Auto-Expiry (Sekunden)
      description: Wann die Auto-Eskalation automatisch wieder aufgehoben wird. Default 1800 (30 Minuten).
  actions:
    save: Speichern
    resetDefaults: Auf Standard zurücksetzen
  success:
    saved: Schwellwerte gespeichert
    savedHint: Änderungen werden beim nächsten Leader-Neustart aktiv.
  errors:
    saveFailed: Speichern fehlgeschlagen
en:
  title: Security & Protection
  description: DoS-defence thresholds. Changes take effect on the next leader restart.
  thresholds:
    title: Detection thresholds
    description: Values are per-second over a sliding 1-second window. Lower values are more sensitive (more false-positive banners), higher values more tolerant.
  l4:
    title: AuthGate rate limit
    warn:
      label: Warn threshold (rejects/sec per peer)
      description: Above this value a banner warning is raised for the affected peer DID. Default 20.
    sample:
      label: Sample threshold (rejects/sec per peer)
      description: Above this value only a fraction of reject logs are written to avoid flooding the database. Default 100.
  l1:
    title: Connection limits (Phase 2)
    global:
      label: Global connections per second
      description: Total limit for inbound QUIC connects. Default 100.
    perSource:
      label: Connections per source endpoint
      description: Limit per source endpoint_id. Default 10. Enforced starting Phase 2.
  l2:
    label: Streams per connection
    description: Maximum concurrently open streams. Default 8.
  l3:
    label: Handshake timeout (seconds)
    description: How long the stream may stay open without completing the handshake. Default 5.
  ddos:
    title: DDoS escalation (Phase 3)
    distinctSources:
      label: Distinct sources threshold
      description: How many distinct sources flooding at once before escalating to DDoS mode. Default 10.
    escalation:
      label: Escalation behaviour
      description: What happens when a DDoS is detected.
      contactsOnly: Accept only contacts
      off: Off (notification only)
    expiry:
      label: Auto-expiry (seconds)
      description: When the auto-escalation is automatically lifted. Default 1800 (30 minutes).
  actions:
    save: Save
    resetDefaults: Reset to defaults
  success:
    saved: Thresholds saved
    savedHint: Changes take effect on the next leader restart.
  errors:
    saveFailed: Failed to save
</i18n>
