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
  --disable-features=AutofillServerCommunication,Translate,BackForwardCache
  about:blank
```

- Deskriptor 3: holzi → Browser, Deskriptor 4: Browser → holzi, jede Nachricht JSON mit `\0` am
  Ende. Windows über die CRT-Vererbung (`lpReserved2`), getestet im Windows-Lauf.
- holzi wartet höchstens 15 s auf die Antwort von `Browser.getVersion`, sonst
  `browser_unavailable`.
- Nach dem Start: `Target.setAutoAttach { autoAttach: true, waitForDebuggerOnStart: true, flatten: true }`:
  jedes neue Ziel wartet auf holzi. Ein Tab, den eine Seite oder ein Rahmen eines Agenten-Tabs öffnet
  (`window.open`, Link oder Formular mit `target=_blank`, auch ohne Opener), gehört der Sitzung dieses
  Agenten-Tabs: holzi hält seine Anfragen an, lässt ihn weiterlaufen und fragt vor seinem ersten
  Dokument (Art `newTab`; Manuell immer, Auto bei nicht erlaubter Website, die Zustimmung erlaubt sie
  einmal; Plan, Ablehnung und mehr als 5 Tabs schließen ihn). Jedes andere neue Ziel läuft sofort
  weiter. `Browser.setPermission` für Kamera,
  Mikrofon, Standort, Benachrichtigungen, Zwischenablage → `denied` (FR-026),
  `Browser.setDownloadBehavior { behavior: "allowAndName", downloadPath: <Arbeitsverzeichnis> }` (R11).

## Je Agenten-Tab

1. `Target.createTarget { url: "about:blank" }`, Anhängen in Flat Mode.
2. `Page.createIsolatedWorld { worldName: "holzi" }` und
   `Page.addScriptToEvaluateOnNewDocument { worldName: "holzi", source: <Leseskript + Markierung> }`.
3. `Runtime.enable` und `Runtime.addBinding { name: "__holzi", executionContextName: "holzi" }`
   für die Knöpfe der Markierung (ohne `Runtime.enable` erreicht die Bindung keine späteren
   Dokumente); nur vertrauenswürdige Ereignisse zählen (R6).
4. `Target.setAutoAttach` hängt auch verschachtelte frame targets an; auf jedem Target aktiviert
   holzi `Fetch.enable` für Dokument-Navigationen (Website- und Wirkungsprüfung vor dem Laden, R9)
   sowie für jede ändernde Anfrage (R5). Jede Freigabe ist an ihre CDP-Request-ID gebunden.

## Eingreifen und Zurückgeben

- Der Nutzer greift ein mit „Eingreifen“ (Sitzungsleiste oder Markierung), auf Bitte des Agenten
  (`browser.takeover.request`, Rückfrage `takeover` mit dem Grund als Angabe des Agenten) oder
  durch eine eigene Eingabe im Tab: Die Markierung meldet jeden vertrauenswürdigen `pointerdown`
  und `keydown` außerhalb der Leiste über die Bindung (`input`). holzi ignoriert die Meldung
  während seiner eigenen `Input.*`-Aufrufe und 500 ms danach (R6).
- Während der Pause endet der laufende Aufruf innerhalb einer Sekunde mit `paused`, jeder weitere
  bekommt `paused`; nur `browser.takeover.request` wartet bis zum Zurückgeben (`returned: true`).
  Die Markierung zeigt „Zurückgeben“: im aktuellen Dokument über `Runtime.evaluate` in holzis Welt
  (höchstens 1 s je Tab, denn eine Seite, die auf ein langsames Dokument wartet, antwortet erst mit
  ihm), in jedem neuen Dokument meldet sich die Markierung über die Bindung (`hello`) und bekommt
  den Zustand der Sitzung. Die gehaltenen Anfragen der Tabs gehen ohne Rückfrage weiter, denn der
  Nutzer handelt; ein neuer Tab, den der Nutzer öffnet, bleibt seiner. Freigaben zum Absenden
  verfallen.
- Zurückgeben prüft die Seite jedes Tabs (Adresse aus `Target.getTargetInfo`, die der Browser auch
  während eines langsamen Dokuments nennt) und die letzte Seite, die er während der Pause zu laden
  begann (sie kann noch unterwegs sein), wie ein neues Dokument (FR-007): außerhalb der Rechte
  fragt holzi; ohne Zustimmung bleibt der Tab beim Nutzer (wie beim Stoppen). Danach steuert der
  Agent wieder.

## Profil

- Arbeitsverzeichnis: Linux `$XDG_RUNTIME_DIR/holzi-agent-browser/<instanz>`, macOS
  `$TMPDIR/holzi-agent-browser/<instanz>`, Windows `%LOCALAPPDATA%\Temp\holzi-agent-browser\<instanz>`;
  Verzeichnis `0700` bzw. nur der Nutzer.
- Start: alte Reste dieser Instanz löschen, `profile.zip` aus dem verschlüsselten Ordner
  `agent-browser/Profil` (data-model.md) lesen und auspacken.
- Ende (Browser beendet, 15 Minuten ohne Sitzung, Vault schließt, holzi beendet sich geordnet):
  Browser beenden (`Browser.close`, nach 5 s Prozess beenden), bleibende Teile packen, als neue
  Version von `profile.zip` in den verschlüsselten Ordner schreiben, Arbeitsverzeichnis löschen.
- In `Default/Preferences` setzt holzi vor jedem Start `credentials_enable_service: false` und
  `autofill.profile_enabled: false` (FR-002).
- Bleibende Teile: `Local State`, `Default/Preferences`, `Default/Network/Cookies` (vor Chromium 96
  `Default/Cookies`), `Default/Local Storage/`, `Default/IndexedDB/`, `Default/Session Storage/`,
  `Default/Network/TransportSecurity` (HSTS), je mit SQLite-Journal. Alles andere entfällt.

## Ende

- Stoppen gibt die Tabs der Sitzung zurück: `Page.stopLoading`, Markierung entfernen,
  `Target.detachFromTarget`; der Tab bleibt offen, ohne Prüfung und Bindung von holzi.
- Vault schließt oder Zugang aus: alle Sitzungen enden (`stopped`), dann Ende wie oben.
- Nutzer schließt das Fenster: Sitzungen enden; der Prozess endet von selbst, holzi sichert das
  Profil.
- Der Browser stürzt ab: Sitzungen enden mit `browser_unavailable`; das Profil des letzten
  geordneten Endes bleibt gültig, das Arbeitsverzeichnis wird gelöscht.
- holzi stürzt ab: das Arbeitsverzeichnis bleibt bis zum nächsten Start liegen und wird dann vor
  allem anderen gelöscht.
