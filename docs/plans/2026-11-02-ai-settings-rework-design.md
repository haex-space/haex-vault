# KI-Einstellungen überarbeiten – Design

Stand: 2026-11-02. Status: Entwurf, mit dem Operator abgestimmt; die Umsetzungsschritte stehen im [Umsetzungsplan](2026-11-02-ai-settings-rework.md).

## Ziel

Die Ansichten unter Einstellungen → KI drei Dinge ändern:

1. Es gibt kein Standardmodell. Der Nutzer wählt immer; ein neuer Chat übernimmt das zuletzt gewählte Modell.
2. "Abo verbinden" und "Modell-Server" werden zu einem Eintrag **Anbieter**: Claude, Codex und OpenAI-kompatible Endpunkte (OpenRouter, Abacus, eigener Server).
3. "Installierte Modelle" und "Modelle herunterladen" werden zu einem Eintrag **Lokale Modelle**. Dort wird gesteuert, welches heruntergeladene Modell aktiv, also im Speicher geladen ist.

## Entscheidungen

| Frage                 | Entscheidung                                                                                                                             |
| --------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| Kein gemerktes Modell | Der Chat startet ohne Modell, das Eingabefeld ist gesperrt, die Auswahl zeigt "Modell wählen". Kein stilles "erstes verfügbares".        |
| Name des Eintrags     | "Anbieter"                                                                                                                               |
| Bedeutung von "aktiv" | Das lokale Modell ist im Speicher geladen. Es hat mit der Modellwahl der Sitzung nichts zu tun.                                          |
| Wie viele aktiv       | Höchstens ein lokales Modell. Heruntergeladen können beliebig viele sein.                                                                |
| Lebensdauer           | Das aktive Modell bleibt im Speicher, bis der Nutzer es in den Einstellungen entlädt. Der Chat kann weder laden noch entladen.           |
| Nach Neustart         | Das aktive lokale Modell wird pro Gerät gemerkt und beim Öffnen der Vault wieder geladen (der bisherige Preload).                        |
| Im Chat               | Das aktive lokale Modell ist ein Modell neben Claude, Codex und den Modellen der Endpunkte. Nur das aktive lokale Modell erscheint dort. |

## 1. Standardmodell entfällt

**Frontend**

- Entfällt: `src/components/settings/DefaultModelSetting.vue`, der Registry-Eintrag `ai.default`, die Route in `appRoutes.ts`, die Actions `settings.models.setDefault` und `settings.models.clearDefault`, die i18n-Texte unter `settings.locations.ai.default` und `settings.default`.
- Onboarding ([\[instance\].vue](../../src/pages/onboarding/[instance].vue)): schreibt nach dem Download nicht mehr `chat.default_model_id`. Es merkt das heruntergeladene Modell mit `ai.active_local_model_id` als aktives lokales Modell vor und setzt `chat.last_active_model_id`, damit der erste Chat es übernimmt. Der Preload lädt das aktive Modell beim Öffnen der Vault (siehe Abschnitt 3).
- Ohne Modell: Auswahl "Modell wählen", Eingabefeld gesperrt.

**Backend** ([default_model.rs](../../src-tauri/src/chat/default_model.rs))

- Die Kette wird: Session → zuletzt gewählt → keins. `DefaultDevice`, `DefaultVault`, `FirstAvailable` und `PREF_DEFAULT_MODEL` entfallen.
- Ein gemerktes lokales Modell, das nicht aktiv ist, zählt als "keins".
- Sprachassistent, Modellaufträge und `send_admission` rufen `resolve_model` ebenfalls auf. Bei "keins" liefern sie eine klare Meldung "kein Modell gewählt". Wie sie sich heute bei `None` verhalten, ist im Plan zu prüfen.
- Alte `chat.default_model_id`-Einträge bleiben als tote Preferences liegen. holzi hat keine Nutzer, es gibt keine Migration.

## 2. Anbieter

- Route `/ai/providers` bleibt, Titel "Anbieter". `ai.servers` entfällt als eigener Punkt. Die Detailseite eines Endpunkts zieht nach `/ai/providers/:providerId`.
- Eine View mit zwei Gruppen:
  - **Abos:** Claude und Codex, mit Verbinden und Modelle neu lesen, wie bisher ([ConnectDelegateProvider.vue](../../src/components/settings/ConnectDelegateProvider.vue)).
  - **Endpunkte:** die Liste der OpenAI-kompatiblen Dienste mit Link zur Detailseite, darunter das Formular zum Hinzufügen ([ModelServersView.vue](../../src/components/settings/ModelServersView.vue)).
- Die beiden Komponenten bleiben bestehen und werden in der neuen View untereinander gesetzt. Kein Neuschreiben.
- Keine Presets für OpenRouter oder Abacus. Der Hinweistext am Adressfeld nennt Beispiel-URLs. Presets können später folgen.
- Beschreibung: "Claude, Codex und OpenAI-kompatible Dienste als Chat-Backend verbinden". Keywords um OpenRouter, Abacus, API-Schlüssel erweitert.
- Die Specs 055 (US5) und 012, die diese Views nennen, werden nachgezogen.

## 3. Lokale Modelle

- Aus `ai.installed` und `ai.download` wird `ai.local` (`/ai/local`). Die HuggingFace-Suche zieht nach `/ai/local/search` und `/ai/local/repo/:owner/:name`.
- **Eine Liste, keine Doppelung.** Oben "Auf diesem Gerät": alle heruntergeladenen Modelle, auch HuggingFace-Funde. Jede Zeile hat "Aktivieren" oder das Badge "Aktiv", dazu Update und Löschen. Die Integritätsprüfung bleibt wie in [InstalledModels.vue](../../src/components/settings/InstalledModels.vue). Darunter "Herunterladen": Suche, laufende Downloads und die empfohlenen Modelle, die noch nicht installiert sind ([DownloadModels.vue](../../src/components/settings/DownloadModels.vue)). Ein fertiger Download wandert in die obere Liste. Der Button "Installiert" in der Katalogliste entfällt.
- **Aktivieren** lädt das Modell in den Speicher und ersetzt dabei das bisher aktive. Reicht die Hardware nicht, scheitert das Laden und die Zeile zeigt den Fehler. **Entladen** gibt den Speicher frei und lässt kein Modell aktiv.
- **Löschen** des aktiven Modells entlädt es zuerst. Eine Bestätigung weist darauf hin.
- **Entladen oder Ersetzen während einer Antwort:** wird abgelehnt mit "Das Modell wird gerade benutzt". Das ist eine Annahme, nicht abgestimmt.

## 4. Backend: das lokale Modell bekommt einen eigenen Slot

Heute hat [ChatState](../../src-tauri/src/chat/session.rs) nur den Slot `session` für das geladene Modell, lokal oder API. Wählt man ein API-Modell, wird ein geladenes lokales verworfen ([model_loading.rs](../../src-tauri/src/chat/model_loading.rs), "If another model was already loaded, it is dropped"). Das passt nicht zum Zielbild.

Neu:

- `ChatState` bekommt neben `session` einen Slot für den geladenen lokalen Motor (Modell-ID, Adapter, Tokenizer-Repo, Kontextfenster, Engine).
- Nur Aktivieren und Entladen auf der Seite "Lokale Modelle" verändern diesen Slot. Neue Befehle dafür; das Pref `ai.active_local_model_id` (Gerät) hält den Merker.
- Wählt der Chat das aktive lokale Modell, baut er seine `session` aus dem Slot und lädt nichts. Wählt er ein API-Modell, bleibt der Slot unberührt.
- Der Preload beim Öffnen der Vault lädt das gemerkte aktive Modell in den Slot. Das Schließen der Vault gibt ihn frei.
- Der Chat kann kein lokales Modell mehr laden. `select_model` und `chat.model.select` behandeln nur noch das aktive lokale Modell und die Modelle der Anbieter. Die Modellliste im Chat enthält von den lokalen nur das aktive.
- Die Stellen, die `session` lesen (rund 240 Zugriffe), bleiben unverändert. Das ist die Begründung für einen zusätzlichen Slot statt eines Umbaus von `session`.

## Fehlerfälle

- Aktives Modell nicht mehr ladbar (Datei fehlt, Integrität): Der Preload meldet den Fehler wie bisher, der Slot bleibt leer, die Seite zeigt "Nicht geladen" mit dem Grund.
- Zuletzt gewähltes Modell ist lokal und nicht aktiv: Der Chat startet ohne Modell.
- Anbieter getrennt, dessen Modell das zuletzt gewählte war: Der Chat startet ohne Modell.

## Tests

- Rust (in `*_tests.rs`): die verkürzte Kette von `choose_model`; Aktivieren ersetzt das aktive Modell; Entladen leert den Slot; ein API-Modell im Chat lässt den Slot unberührt; Entladen während einer Antwort wird abgelehnt.
- Frontend: Check-Skripte `check:*` laufen alle; die Registry-Prüfung deckt die neuen und entfernten Orte ab; E2E für den Weg "Modell aktivieren → im Chat wählbar" und "kein Modell → Eingabe gesperrt".

## Offen für den Plan

- Wizard setzt zusätzlich das zuletzt gewählte Modell (Annahme: ja).
- Verhalten von Sprachassistent, Modellaufträgen und `send_admission` bei "kein Modell".
- Entladen oder Ersetzen während einer Antwort: ablehnen (Annahme) oder die Antwort abbrechen.
