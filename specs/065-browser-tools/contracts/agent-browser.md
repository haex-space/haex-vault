# Contract: Agenten-Browser (Prozess, Verbindung, Profil)

**Feature**: [../spec.md](../spec.md) | **Research**: R2, R3, R6, R9, R13

## Finden

Reihenfolge je Betriebssystem, erster Treffer gewinnt; ein in „KI → Browser“ gewählter Pfad geht
vor:

- **Linux**: `chromium`, `chromium-browser`, `google-chrome`, `google-chrome-stable`,
  `microsoft-edge`, `brave-browser` auf `PATH`; Flatpak wird nicht unterstützt (keine Pipe über die
  Sandbox).
- **macOS**: `/Applications/{Google Chrome,Chromium,Microsoft Edge,Brave Browser}.app/Contents/MacOS/…`
  und dasselbe unter `~/Applications`.
- **Windows**: `%ProgramFiles%`, `%ProgramFiles(x86)%`, `%LOCALAPPDATA%` je
  `Google\Chrome\Application\chrome.exe`, `Microsoft\Edge\Application\msedge.exe`,
  `BraveSoftware\Brave-Browser\Application\brave.exe`, `Chromium\Application\chrome.exe`.

Mindestversion: Chromium 120 (`Browser.getVersion`); ältere gelten als nicht gefunden.

Nichts gefunden: Angebot „Chrome for Testing“ in der Version, die holzi kennt (Größe, Quelle,
Prüfsumme sichtbar), Ablage im Datenverzeichnis von holzi, für alle Vaults des Geräts geteilt.

## Start

```text
<browser> --remote-debugging-pipe --user-data-dir=<Arbeitsverzeichnis>
  --no-first-run --no-default-browser-check --disable-sync --disable-extensions
  --disable-background-networking --disable-component-update
  --password-store=basic (Linux) | --use-mock-keychain (macOS)
  --disable-features=AutofillServerCommunication,Translate
  about:blank
```

- Deskriptor 3: holzi → Browser, Deskriptor 4: Browser → holzi, jede Nachricht JSON mit `\0` am
  Ende. Windows über die CRT-Vererbung (`lpReserved2`), getestet im Windows-Lauf.
- holzi wartet höchstens 15 s auf die Antwort von `Browser.getVersion`, sonst
  `browser_unavailable`.
- Nach dem Start: `Target.setAutoAttach { flatten: true }`, `Browser.setPermission` für Kamera,
  Mikrofon, Standort, Benachrichtigungen, Zwischenablage → `denied` (FR-026),
  `Browser.setDownloadBehavior { behavior: "allowAndName" }` (R11).

## Je Agenten-Tab

1. `Target.createTarget { url: "about:blank" }`, Anhängen in Flat Mode.
2. `Page.createIsolatedWorld { worldName: "holzi" }` und
   `Page.addScriptToEvaluateOnNewDocument { worldName: "holzi", source: <Leseskript + Markierung> }`.
3. `Runtime.addBinding { name: "__holzi", executionContextName: "holzi" }` für die Knöpfe der
   Markierung; nur vertrauenswürdige Ereignisse zählen (R6).
4. `Fetch.enable` mit Mustern `{ resourceType: Document }` (Website-Prüfung vor dem Laden, R9) und
   für ändernde Anfragen im Zeitfenster aus R5.

## Profil

- Arbeitsverzeichnis: Linux `$XDG_RUNTIME_DIR/holzi-agent-browser/<instanz>`, macOS
  `$TMPDIR/holzi-agent-browser/<instanz>`, Windows `%LOCALAPPDATA%\Temp\holzi-agent-browser\<instanz>`;
  Verzeichnis `0700` bzw. nur der Nutzer.
- Start: alte Reste dieser Instanz löschen, Archiv `profile.hxbp` (data-model.md) entschlüsseln
  und auspacken.
- Ende (Browser beendet, 15 Minuten ohne Sitzung, Vault schließt, holzi beendet sich geordnet):
  Browser beenden (`Browser.close`, nach 5 s Prozess beenden), bleibende Teile packen,
  verschlüsseln, Archiv atomar ersetzen (schreiben nach `.tmp`, `fsync`, umbenennen),
  Arbeitsverzeichnis löschen.
- Bleibende Teile: `Default/Cookies`, `Default/Local Storage/`, `Default/IndexedDB/`,
  `Default/Session Storage/`, `Default/Preferences`, `Local State`. Alles andere entfällt.

## Ende

- Vault schließt oder Zugang aus: alle Sitzungen enden (`stopped`), dann Ende wie oben.
- Nutzer schließt das Fenster: Sitzungen enden; der Prozess endet von selbst, holzi sichert das
  Profil.
- Der Browser stürzt ab: Sitzungen enden mit `browser_unavailable`; das Profil des letzten
  geordneten Endes bleibt gültig, das Arbeitsverzeichnis wird gelöscht.
