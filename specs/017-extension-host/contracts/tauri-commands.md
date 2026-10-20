# Vertrag: Tauri-Commands und Ereignisse für holzis Oberfläche

Nur holzis eigenes Frontend ruft diese Commands auf. Eine Erweiterung erreicht ausschließlich
`extension_bridge_call` und nur über die Weiterleitung im Frontend ([bridge.md](./bridge.md)). Alle Commands
laufen über `VaultDb` und sind nach dem Schließen der Vault gesperrt (ADR-0003). Typen per ts-rs nach
`src/types/bindings/`. Fehlerart `HolziError::Extension*` ohne Daten der Erweiterung im Text.

## Installation und Verwaltung (L1, L3)

| Command                     | Eingabe                                           | Ausgabe                                                                                                                                                                                                                    | Hinweise                                                                                                                   |
| --------------------------- | ------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `extension_install_preview` | `{path}` (aus dem Datei-Dialog)                   | `InstallPreview {name, displayName, version, publisherFingerprint, signatureValid, error?, declared: DeclaredPermission[], unsupportedCategories, existing?: {version, isDowngrade, newPermissions}, replacesSameVersion}` | liest die Datei in Rust; kein Schreiben                                                                                    |
| `extension_install`         | `{path, accepted: PermissionChoice[], confirmed}` | `ExtensionSummary`                                                                                                                                                                                                         | prüft erneut, schreibt BLOBs einzeln, dann Registry                                                                        |
| `extension_list`            | –                                                 | `ExtensionSummary[]` (inkl. Zustand je Gerät, wirksame Fassung; entfernte nur mit behaltenen Daten, dann mit `keptDataBytes` auf diesem Gerät)                                                                             | auch Erweiterungen mit behaltenen Daten                                                                                    |
| `extension_set_enabled`     | `{extensionId, enabled}`                          | –                                                                                                                                                                                                                          | L3; gilt auf allen Geräten, Tabs schließen sich (FR-007, FR-039)                                                           |
| `extension_remove`          | `{extensionId, deleteData}`                       | –                                                                                                                                                                                                                          | L2 (Befehl), Einstellungen L3; setzt Grabstein mit eigenem `purge_hlc` (R11); bei „Daten behalten“ bleiben die Migrationen |
| `extension_purge_kept_data` | `{extensionId}`                                   | –                                                                                                                                                                                                                          | L3; behaltene Daten entfernter Erweiterungen, über einen neuen `purge_hlc` auf allen Geräten                               |
| `extension_icon`            | `{extensionId}`                                   | `data:`-URL                                                                                                                                                                                                                | für Launcher und Einstellungen                                                                                             |

## Berechtigungen (L1)

| Command                        | Eingabe                                                                     | Ausgabe                                                                    |
| ------------------------------ | --------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| `extension_permissions_list`   | `{extensionId}`                                                             | `PermissionView[]` (gemerkt mit Geltungsbereich und Gerätename, vorläufig) |
| `extension_permission_set`     | `{extensionId, kind, action, target, status, replaces?}`                    | –                                                                          |
| `extension_permission_remove`  | `{permissionId}` oder `{temporaryKey}`                                      | –                                                                          |
| `extension_permission_resolve` | `{requestId, decision: allow \| deny, remember, model?: {modelId, effort}}` | – (`model`: Wahl des Nutzers bei einer Frage nach einem Modell, Spec 055)  |
| `extension_permission_cancel`  | `{requestId}` (Dialog geschlossen; die nächste gleiche Frage kommt wieder)  | –                                                                          |

Modelle einer Erweiterung (Spec 055 US2): `extension_model_grants_list {extensionId}` →
`ExtensionModelEntry[]` (erst die erlaubten in ihrer Rangfolge, dann die übrigen nach Namen);
`extension_model_grants_set {args: {extensionId, modelId, effort}}` erlaubt ein Modell bis zu diesem Aufwand und
ersetzt jede frühere Zeile des Modells; `extension_model_grants_revoke {args: {extensionId, modelId}}` nimmt es
zurück, auch eine Entscheidung für die Sitzung; `extension_model_grants_reorder {args: {extensionId, modelIds}}`
ordnet die erlaubten. Die Liste der Berechtigungen in den Einstellungen zeigt die Modelle deshalb nicht.

Einwilligungen für Modellaufträge (Spec 055 US4) sind Berechtigungen der Art `modelDestination` mit der
Anbieterkennung als Ziel; sie laufen über die Befehle oben. `PermissionView.destination` und
`PermissionRequestEvent.destination` nennen Anbieter, Host und Ort, die Frage dazu mit `transfer`, was der Schritt
senden würde.

Ereignis `extension-permission-request` (siehe [permissions.md](./permissions.md)).

## Grenzwerte, Protokolle, Entwicklermodus (L3)

| Command                                         | Eingabe                                   | Ausgabe                                                                                                    |
| ----------------------------------------------- | ----------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `extension_limits_get` / `extension_limits_set` | `{extensionId}` / `{extensionId, limits}` | `ExtensionLimitsView {values, min, max}` / –                                                               |
| `extension_logs_read`                           | `{extensionId, level?, limit, before?}`   | `LogEntry[]`                                                                                               |
| `extension_dev_mode_get`                        | –                                         | `DevModeState`; ist der Modus an und das Dokument ohne Freigabe geladen, lädt das Fenster einmal neu (R16) |
| `extension_dev_mode_set`                        | `{enabled}`                               | `DevModeState {enabled, framesAllowed}` (gerätebezogene Einstellung; das Fenster lädt danach neu)          |
| `extension_dev_load`                            | `{projectPath}`                           | `InstallPreview` (abgelehnt mit Grund, solange der Modus aus ist oder das Präfix belegt ist)               |
| `extension_dev_confirm`                         | `{projectPath, accepted}`                 | Kennung der Entwicklungsfassung                                                                            |
| `extension_dev_unload`                          | `{extensionId}`                           | – (löscht Registrierung, Berechtigungen, Speicher und Tabellen)                                            |

`Limits` hat `maxRows`, `maxConcurrent`, `maxSqlBytes`, `timeoutMs` und `maxResponseBytes`. Nur holzi prüft die
Grenzen und liefert sie mit, damit die Einstellungen sie anzeigen: Zeilen 1 bis 1.000.000, gleichzeitige Anfragen
1 bis 100, Anweisung 1.000 Bytes bis 16 MiB, Laufzeit 100 bis 60.000 ms, Antwort 64 KiB bis 256 MiB. Ein Wert
außerhalb wird mit `InvalidInput` abgelehnt, nichts wird geschrieben.

## Rahmen (L1)

| Command                    | Eingabe                       | Ausgabe                                                                                                      |
| -------------------------- | ----------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `extension_frame_open`     | `{extensionId, tabId}`        | `{frame, url}` (URL mit Start-Token) oder Fehler `disabled` / `not_ready {status}`                           |
| `extension_frame_close`    | `{frame}`                     | – (beendet Beobachtungen und Shells, wenn es der letzte Rahmen war)                                          |
| `extension_frame_reloaded` | `{frame}`                     | – (eine neue Seite im Rahmen: dessen Shells warten nicht mehr auf seine Quittungen, bis er wieder quittiert) |
| `extension_bridge_call`    | `{frame, id, method, params}` | `{id, result}` oder `{id, error}` (SDK-Form)                                                                 |

Ereignis `extension-frame-event {frame, type, data, timestamp}`.

`extension_host_context_set {theme, locale}` (nur holzis Fenster): Farbschema (`light` | `dark` | `system`) und
Sprache, wie holzis Oberfläche sie anwendet; Grundlage von `extension_context_get`. So liest nur das Frontend
die Einstellungen, und Rust kennt sie trotzdem.

## Weitere Ereignisse

| Ereignis                       | Daten                                   | Zweck                                                                                                                                                               |
| ------------------------------ | --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `extensions-changed`           | `{extensionIds}`                        | Liste neu laden (Launcher, Einstellungen, App-Liste des wm)                                                                                                         |
| `extension-status-changed`     | `{extensionId, status, error?, reload}` | Anzeige „wird übertragen“, Fehler; `reload`: die wirksame Fassung einer laufenden Erweiterung hat gewechselt, offene Tabs laden neu (FR-038)                        |
| `extension-notification-click` | `{extensionId, path?}`                  | Klick auf eine Benachrichtigung einer Erweiterung: ihren Tab nach vorn holen oder sie öffnen (FR-052); die Erweiterung selbst hört `haextension:notification:click` |
