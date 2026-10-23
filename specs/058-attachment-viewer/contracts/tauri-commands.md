# Contract: Tauri-Commands für Anhang-Ansicht und System-App

**Feature**: [spec.md](../spec.md) | **Research**: [research.md](../research.md)

Alle drei Commands sind nur für das Fenster von holzi registriert, prüfen `require_user` (FR-014)
und erscheinen weder in der Brücke für Erweiterungen noch unter den Werkzeugen für Agenten. Kein
Fehler und kein Log nennt Inhalt oder Pfad (FR-013); der Dateiname erscheint in Logs nur
`Redacted`.

## `passwords_attachment_text`

```ts
invoke<TextContent>('passwords_attachment_text', {
  args: { attachmentId: string },
})
```

| Ergebnis                              | Wann                                                     |
| ------------------------------------- | -------------------------------------------------------- |
| `TextContent { text, truncated }`     | Art `text`; `truncated` ab mehr als 5 MB                 |
| `InvalidInput { reason: "binary" }`   | NUL in den ersten 8 KiB (044)                            |
| `InvalidInput { reason: "not_text" }` | Die Endung ist keine Textendung von `viewer_kind`        |
| `PasswordsNotFound`                   | Anhang entfernt oder Binärzeile noch nicht auf dem Gerät |
| Zugriffsfehler aus `require_user`     | Aufrufer ist nicht der Nutzer                            |

## `passwords_attachment_open`

```ts
invoke<string>('passwords_attachment_open', {
  args: { attachmentId: string, tabId: string },
})
```

Meldet die Bytes eines PDF-Anhangs beim Media-Server an und gibt die URL zurück
(`http://127.0.0.1:<port>/<token>`). Das Fenster gibt sie mit `files_release` frei, wenn die Ansicht
schließt; sonst endet sie mit dem Tab (`files_release_tab`) oder der Vault.

| Ergebnis                             | Wann                       |
| ------------------------------------ | -------------------------- |
| URL                                  | Art `pdf`                  |
| `InvalidInput { reason: "not_pdf" }` | Die Endung ist nicht `pdf` |
| `PasswordsNotFound`                  | wie oben                   |

Ob das PDF lesbar ist, entscheidet pdf.js im Fenster; ein unlesbares PDF meldet `PdfViewer` mit
`failed` (US2 Szenario 2).

## `passwords_attachment_open_system`

```ts
type OpenSystemOutcome = 'opened' | 'needsNotice'
invoke<OpenSystemOutcome>('passwords_attachment_open_system', {
  args: { attachmentId: string, confirmed: boolean, dontAskAgain: boolean },
})
```

| Ergebnis                                 | Wann                                                                                           |
| ---------------------------------------- | ---------------------------------------------------------------------------------------------- |
| `'needsNotice'`                          | `confirmed = false` und keine Bestätigung auf diesem Gerät; nichts gelesen, nichts geschrieben |
| `'opened'`                               | Kopie geschrieben, System-App gestartet; mit `dontAskAgain` ist die Bestätigung gespeichert    |
| `InvalidInput { reason: "unsupported" }` | mobile Plattform (`cfg(mobile)`); nichts gelesen                                               |
| `InvalidInput { reason: "unwritable" }`  | Kopie ließ sich nicht schreiben; Ordner der Kopie entfernt                                     |
| `InvalidInput { reason: "no_app" }`      | Keine App für diese Datei; Kopie sofort entfernt (FR-012)                                      |
| `PasswordsNotFound`                      | wie oben                                                                                       |

`OpenSystemOutcome` ist ein ts-rs-Binding (`src/types/bindings/OpenSystemOutcome.ts`). Die Argumente
sind wie bei den übrigen Anhang-Befehlen in `args` verpackt: `AttachmentIdArgs` (vorhanden) für
`passwords_attachment_text`, neu `AttachmentOpenArgs` und `AttachmentOpenSystemArgs`.

## Unverändert wiederverwendet

- `files_release(url)`, `files_release_tab(tabId)` (044)
- `passwords_attachment_save`, `passwords_attachment_preview` (034, 036)
