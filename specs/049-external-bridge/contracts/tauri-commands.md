# Vertrag: Commands, Aktionen und Ereignisse im Fenster

**Spec**: [../spec.md](../spec.md) | **Research**: R10, R13 bis R15

Typen erzeugt `ts-rs` nach `src/types/bindings/`. Alle Commands antworten auf Android mit
`NotAvailable` (FR-043).

## Katalogaktionen (Einstellungen, nur Fenster)

Alle Aktionen haben den Bereich `guardrails` und sind damit `agentCallable: false`
(`src/lib/actions/settingsActions.ts:100`, FR-013). Jede ruft genau einen Command.

| Aktion                                  | Eingabe                                         | Command                         | Wirkung                                                                                            |
| --------------------------------------- | ----------------------------------------------- | ------------------------------- | -------------------------------------------------------------------------------------------------- |
| `settings.browserBridge.set`            | `{ enabled: boolean, port: number }`            | `browser_bridge_configure`      | speichert beides geräteeigen; startet, stoppt oder startet die Bridge neu (R14)                    |
| `settings.browserBridge.client.update`  | `{ clientId, access, scopeTags?, defaultTag? }` | `browser_bridge_client_update`  | ändert eine gemerkte oder vorläufige Zulassung; gilt ab der nächsten Anfrage (FR-038)              |
| `settings.browserBridge.client.revoke`  | `{ clientId }`                                  | `browser_bridge_client_revoke`  | löscht die Zulassung; `authorizationUpdate(false)` an offene Verbindungen, dann Schließen (FR-011) |
| `settings.browserBridge.client.unblock` | `{ clientId }`                                  | `browser_bridge_client_unblock` | hebt die Sperre auf; beim nächsten Verbinden fragt holzi                                           |

Die Client-Aktionen und ihre Commands antworten mit der neuen `BridgeOverview`. Im Schema der
Aktion fehlen `scopeTags` und `defaultTag` für `null`, weil das Aktionsschema kein `null` kennt.
Eine Änderung vergisst die Antworten zum Schreiben aus dieser Sitzung, denn der Nutzer hat neu
entschieden.

**Prüfung im Command**:

- `access` ist `read` oder `readWrite`.
- `scopeTags` ist `null` (alle Einträge) oder eine nicht leere Liste.
- `defaultTag` liegt im Bereich und ist bei `readWrite` mit `scopeTags` gesetzt.
- Nichts davon darf über die gespeicherte Erklärung hinausgehen.
- `port` liegt zwischen 1024 und 65535.
- `clientId` nennt eine Zulassung (`update`, `revoke`) oder eine Sperre (`unblock`), gemerkt oder
  vorläufig; sonst `BrowserBridgeClientNotFound`. Ohne laufende Bridge ändern die Commands nur die
  gemerkten Entscheidungen.

## Lesende Commands

### `browser_bridge_overview() -> BridgeOverview`

```ts
type BridgeOverview = {
  enabled: boolean
  port: number
  state: 'off' | 'running' | 'portInUse' | 'noVault' | 'notAvailable'
  clients: BridgeClientView[] // zugelassen, gemerkt und vorläufig
  blocked: BridgeClientView[] // gesperrt, gemerkt und vorläufig
}
type BridgeClientView = {
  clientId: string // Fingerabdruck; die Anzeige gruppiert in Vierergruppen
  clientName: string // Angabe des Clients
  origin: string
  access: 'read' | 'readWrite' | null
  scopeTags: string[] | null
  defaultTag: string | null
  // die weiteste Freigabe der Erklärung; der Editor bietet nichts darüber hinaus
  declared: { access: 'read' | 'readWrite'; scopeTags: string[] | null } | null
  declaresWrite: boolean
  remembered: boolean
  connected: boolean
  decidedAt: number
  lastSeenAt: number | null
}
```

Eine vorläufige Entscheidung verdeckt eine gemerkte für denselben Client, und eine Sperre verdeckt
eine Zulassung, so wie `approvals::decide` sie gewichtet. Ohne laufende Bridge stehen nur die
gemerkten Entscheidungen in den Listen, alle mit `connected: false`.

## Rückfragen

### Ereignis `browser-bridge-prompt` (Rust → Fenster)

```ts
// Rust-Typ BridgePromptEvent; die Frage selbst ist BridgeQuestion.
type BridgePromptEvent = {
  promptId: string
  expiresAt: number
  question: BridgeQuestion
}
type BridgeQuestion = { client: BridgeClientRef } & (
  | {
      kind: 'approval'
      declared: { access: 'read' | 'readWrite'; scopeTags: string[] | null }
      declaresWrite: boolean
    }
  | {
      kind: 'write'
      method: 'create-item' | 'passkey-create'
      scopeTags: string[] | null
    }
  | {
      kind: 'presence'
      ceremony: 'create' | 'get'
      rpId: string
      rpName: string | null
      origin: string
      userName: string | null
      requireUv: boolean
      choices: PresenceChoice[]
    } // create: 'new' + Einträge; get: Passkeys
)
type BridgeClientRef = { clientId: string; clientName: string; origin: string }
type PresenceChoice =
  | { kind: 'newEntry' }
  | { kind: 'entry'; itemId: string; title: string; url: string | null }
  | {
      kind: 'passkey'
      passkeyId: string
      userName: string
      itemTitle: string | null
    }
```

- **Fehlender Name**: Bei `kind: 'approval'` mit leerem `clientName` zeigt das Fenster „ohne
  Namen“.
- **Kennzeichnung**: Der Name erscheint immer als Angabe des Clients (FR-008).

### Ereignis `browser-bridge-prompt-cancelled` (Rust → Fenster)

`{ promptId }`. Das Fenster schließt den Dialog. Gründe sind Ablauf, Ende der Verbindung,
Ausschalten und das Schließen der Vault.

### Command `browser_bridge_answer(promptId, answer) -> ()`

```ts
type BridgeAnswer =
  | { kind: 'approval'; allow: false; remember: boolean }
  | {
      kind: 'approval'
      allow: true
      remember: boolean
      access: 'read' | 'readWrite'
      scopeTags: string[] | null
      defaultTag: string | null
    }
  | {
      kind: 'write'
      allow: boolean
      remember: boolean
      defaultTag: string | null
    }
  | { kind: 'presence'; confirm: false }
  | {
      kind: 'presence'
      confirm: true
      choice: PresenceChoice
      passphrase: string | null
    }
```

Regeln:

- **Unbekannte Rückfrage**: Ist `promptId` unbekannt oder schon beantwortet, gibt es keinen Fehler
  und keine Wirkung.
- **Gleiche Prüfung wie in den Einstellungen**: Eine `approval`-Antwort mit `allow: true` prüft
  Rust wie `browser_bridge_client_update`. Eine ungültige Antwort gilt als „Ablehnen“.
- **`write`**: `defaultTag` ist Pflicht, wenn der Bereich aus Tags besteht und Anlegen erlaubt
  wird (FR-015).
- **`presence`**:
  - `choice` muss aus `choices` der Rückfrage stammen.
  - `passphrase` ist Pflicht genau bei `requireUv`. Rust prüft es mit `verify_current` (R12).
  - Ein falsches Passwort wirkt wie `confirm: false`. Das Fenster zeigt dann „Passwort falsch“ und
    schließt den Dialog.

### Ereignis `browser-bridge-changed` (Rust → Fenster)

Kein Inhalt. Die Einstellungen laden `browser_bridge_overview` neu. Rust sendet es, wenn sich der
Zustand, eine Zulassung, eine Sperre oder `connected` ändert.

### Command `browser_bridge_notify(title, body) -> ()`

Eine Systembenachrichtigung, die zur Rückfrage führt, wenn holzi nicht nach vorn kommen konnte
(Edge Case „holzi-Fenster minimiert“). Das Fenster schickt den Text in der Sprache des Nutzers, weil
Rust keine eigenen Texte hat.

## Plattform

`platform_capabilities` liefert zusätzlich `browserBridge: boolean`
(`useDeviceCapabilities.ts`).
