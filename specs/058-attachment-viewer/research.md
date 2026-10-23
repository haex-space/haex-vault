# Research: Dateianhänge im Passwortmanager anzeigen und öffnen

**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Date**: 2026-10-23

Jede Entscheidung nennt die Stelle im Code, an der sie anknüpft. Es gab keine offenen
„NEEDS CLARIFICATION“ im Technical Context; die Punkte unten klären, wie die Spec auf den vorhandenen
Code aus 034, 036, 044 und 048 trifft.

## R1 – Eine Regel für „ist Text“ und „ist PDF“

- **Decision**: `fileKind` in `src/lib/passwords/format.ts` entscheidet Bilder weiter über
  `imageMime` (nur die Formate, die Vorschau und Lightbox lesen: png, jpg, jpeg, gif, webp) und
  übernimmt Text und PDF aus `viewerKind` (`src/lib/files/viewerKind.ts`, Spiegel von
  `src-tauri/src/files/kind.rs::viewer_kind`). `TEXT_EXTENSIONS` (format.ts:119) entfällt. In Rust
  prüft der neue Text-Befehl die Art mit `viewer_kind`, nicht mit einer eigenen Liste.
- **Rationale**: FR-002 verbietet eine zweite Liste; `viewerKind` und `viewer_kind` sind schon durch
  `scripts/check-files-viewer.ts` gegeneinander abgesichert.
- **Alternatives considered**: `fileKind` ganz durch `viewerKind` ersetzen – verworfen, weil
  `viewerKind` auch svg, bmp und avif als Bild führt, die `attachment_preview`
  (binaries.rs:253) nicht ausliefert; die Lightbox bliebe für sie leer.

## R2 – Text: Bytes aus der Vault durch den Leser von 044

- **Decision**: Neuer Command `passwords_attachment_text(attachmentId) -> TextContent`. Er liest die
  Bytes mit `binaries::attachment_data`, prüft `viewer_kind(name) == Text` und gibt sie an
  `files::local::text::text_from(bytes, TEXT_LIMIT)` (text.rs:41). Die Grenze von 5 MB, das
  Abschneiden, die Erkennung von Binärdateien (NUL in den ersten 8 KiB) und das Ersetzen ungültiger
  UTF-8-Folgen kommen damit von genau einer Stelle. `text_from` und `TEXT_LIMIT` sind schon `pub`
  (`files::local` und `files::local::text` sind `pub mod`), es braucht keine Änderung an der
  Sichtbarkeit.
- **Fehler**: `FilesErrorCode::Binary` wird zu `HolziError::InvalidInput { reason: "binary" }`, eine
  andere Art zu `reason: "not_text"`; ein fehlender Anhang (auch einer, dessen Binärzeile noch nicht
  angekommen ist) bleibt `PasswordsNotFound`. `useErrorString` bekommt die Texte dafür.
- **Rationale**: SC-003 (gleiches Verhalten wie im Datei-Viewer) ohne zweite Umsetzung; nur der
  Nutzer (`require_user`) erreicht den Befehl (FR-014).
- **Alternatives considered**: Bytes über `attachment_preview` ans Fenster und dort mit
  `TextDecoder` dekodieren – verworfen: zweite Umsetzung von Grenze und Binärerkennung, und bis zu
  25 MiB gingen über die IPC, obwohl höchstens 5 MB gezeigt werden.

## R3 – PDF: der Media-Server aus 044 mit einer Quelle im Arbeitsspeicher

- **Decision**: Neuer Command `passwords_attachment_open(attachmentId, tabId) -> String` (URL). Er
  liest die Bytes (höchstens 25 MiB, 034 FR-020), prüft `viewer_kind(name) == Pdf` und meldet sie beim
  `MediaServer` (`files/media/server.rs:81`) als `BytesSource` an. Das Fenster zeigt die URL mit dem
  unveränderten `LazyFilesPdfViewer` und gibt sie beim Schließen mit `files_release` frei
  (`useFiles().releaseAsync`). Beim Sperren oder Schließen endet der Media-Server mit dem Token der
  Vault (ADR 0003), damit auch die Bytes (FR-006, FR-008).
- **`BytesSource`**: neu in `src-tauri/src/files/streaming.rs`, hält `Arc<[u8]>` und den Medientyp,
  `open_range` gibt einen `Cursor` über den Ausschnitt. Kandidat aus graphify:
  `platform_probe::Fixture` (platform_probe.rs:170) macht dasselbe für `&'static [u8]`. Vorschlag: `Fixture`
  wird zu einem Aufruf von `BytesSource::new(Arc::from(bytes), content_type)`; das spart dort die
  eigene `StreamingSource`-Umsetzung (~25 Zeilen). **Wartet auf die Zustimmung des Betreibers**
  (graphify-first-authoring); ohne Zustimmung bleibt `Fixture` unverändert und `BytesSource` entsteht
  trotzdem, weil `Fixture` nur statische Bytes kann.
- **Rationale**: `PdfViewer.vue` erwartet eine URL und lädt mit Bereichsanfragen
  (`lib/files/pdf.ts:30`); der Media-Server ist der Weg, auf dem holzi das schon für entschlüsselte
  Dateien tut (`EncryptedFileSource`). Keine Datei auf dem Gerät (FR-006).
- **Alternatives considered**: Blob-URL im Fenster aus `attachment_preview` – verworfen: zweiter
  Ladeweg für pdf.js, und die CSP für `connect-src blob:` müsste geprüft und geöffnet werden;
  `PdfViewer` mit `data` statt `url` – verworfen, weil der Viewer dann zwei Eingänge bekäme.

## R4 – „Mit System-App öffnen“: der Ort und die Aufräumregeln aus 048

- **Decision**: Neuer Command `passwords_attachment_open_system(attachmentId, confirmed,
dontAskAgain) -> OpenSystemOutcome` (`opened` | `needsNotice`). Ablauf:
  1. Ohne `confirmed` liest er die Geräte-Einstellung `passwords.attachmentOpenAck`
     (`preferences`, `PrefScope::Device(<dieses Gerät>)`); fehlt sie, antwortet er `needsNotice`,
     ohne etwas zu entschlüsseln.
  2. Mit `confirmed && dontAskAgain` setzt er die Einstellung.
  3. Er schreibt die Bytes nach `FilesState::opened_encrypted/<uuid>/<sicherer Name>`, also in
     denselben Ordner wie 048 (`files-opened-encrypted`), der schon beim Schließen der Vault
     (`forget_on_close`) und beim Start (`FilesState::from_app`) geleert wird. Der Command ruft
     `files.forget_on_close(state.gate())` auf, damit das Aufräumen auch scharf ist, wenn die
     Dateien-App in dieser Sitzung nie offen war (der Aufruf ist idempotent, state.rs:70).
  4. Er öffnet mit `tauri_plugin_opener::open_path`; scheitert das, löscht er den Ordner der Kopie
     sofort (FR-012) und antwortet mit einem Fehler ohne Pfad (FR-013).
  5. Auf mobilen Plattformen antwortet er `InvalidInput { reason: "unsupported" }`, bevor er etwas liest; das Fenster zeigt die
     Aktion dort nicht (`useDeviceCapabilities().isAndroid`, wie `Viewer.vue`).
- **Sicherer Dateiname**: Der Name entsteht in Rust, weil dort die Datei entsteht.
  `binaries::sanitize_file_name` (binaries.rs:42) ersetzt Trenner und Steuerzeichen, kennt aber die
  Zeichen nicht, die Windows ablehnt. Die Kopie braucht einen Namen, der auf jedem System angelegt
  werden kann (holzi läuft auf Windows, macOS und Linux; eine Lücke ist ein Fehler). Neue Funktion
  `copy_file_name(name)` im neuen Modul: `sanitize_file_name`, dann jedes Zeichen aus
  `files::local::edit::FORBIDDEN` (edit.rs:17, dafür `pub(crate)`) zu `_`, ein Punkt oder Leerzeichen
  am Ende zu `_`, und ein Stamm, der ein reservierter Gerätename von Windows ist (CON, PRN, AUX, NUL,
  COM1–9, LPT1–9, ohne Rücksicht auf Groß- und Kleinschreibung), bekommt ein `_` vorangestellt.
  `sanitize_file_name` selbst bleibt unverändert, weil sie die gespeicherten Namen bildet (034
  FR-021); `check_name` in edit.rs prüft nur und lehnt ab, passt also nicht für einen Namen, der immer
  gelingen muss.
- **Rationale**: 048 FR-028 ist die Vorlage, die der Betreiber gewählt hat; derselbe Ordner heißt
  dieselben Aufräumregeln ohne zweite Sweep-Logik. Die Einstellung in `preferences` braucht keine
  Migration; sie ist pro Gerät geschlüsselt und wirkt nur dort (FR-010).
- **Alternatives considered**: eigener Ordner `passwords-opened` mit eigenem Sweep – verworfen
  (doppelte Aufräumlogik); eine `_no_sync`-Tabelle für die Bestätigung – verworfen (Migration für ein
  einzelnes Bit; die Geräte-Einstellung reicht); Hinweis-Bestätigung je Anhang – verworfen, die Spec
  verlangt „für alle Anhänge dieser Vault auf diesem Gerät“.
- **Befund am Rande**: `files_open_system` (view.rs:336) lässt die Kopie liegen, wenn `open_path`
  scheitert. Für Anhänge gilt FR-012; für 048 ist das ein eigener Fix außerhalb dieser Spec.

## R5 – Oberfläche: gemeinsame Teile statt einer Kopie von `Viewer.vue`

- **Decision**:
  - Neue Komponente `FilesTextContent.vue` (Hinweis „abgeschnitten“ + `<pre>`), herausgelöst aus
    `Viewer.vue` (Zeilen 192–203); `Viewer.vue` und die Anhang-Ansicht nutzen sie (SC-003).
  - Neue Komponente `PasswordsAttachmentViewer.vue`: Dialog über dem Eintrag mit Kopfzeile
    (Schließen, Name, Speichern unter, Mit System-App öffnen) wie `Viewer.vue`, Inhalt
    `FilesTextContent` oder `LazyFilesPdfViewer`, sonst der Hinweis mit den Aktionen. Escape schließt,
    der Fokus geht an die Karte zurück (FR-005).
  - `OpenSystemNotice.vue` wird unverändert wiederverwendet; sein Text ist allgemein („Entschlüsselte
    Kopie auf diesem Gerät …“).
  - `AttachmentCard.vue`: `activate` öffnet für `text` und `pdf` die Ansicht; neue Schaltfläche „Mit
    System-App öffnen“ (nicht auf Android). `Attachments.vue` hält den Zustand der Ansicht und den
    Hinweis.
- **Rationale**: `Viewer.vue` ist an `SourceRef`, `Entry` und Geschwister gebunden (Ordnerlogik,
  Blättern); die Anhang-Ansicht braucht davon nichts. Was beide gleich zeigen, liegt an einer Stelle.
- **Alternatives considered**: `Viewer.vue` mit einer Quelle „Anhang“ erweitern – verworfen, die
  Ordner- und Geschwisterlogik würde mit Sonderfällen durchsetzt; Anhang-Ansicht als eigenes Fenster
  des WM – verworfen, die Spec verlangt eine Ansicht über dem Eintrag mit Fokus-Rückgabe.

## R6 – Tests

- **Decision**:
  - Rust: `passwords/attachment_view_tests.rs` – Text (Grenze, Binärdatei, ungültiges UTF-8, falsche
    Art, fehlender Anhang, nur `Caller::User`), PDF-Anmeldung beim Media-Server (falsche Art, Range
    über `BytesSource`), System-App ohne Bestätigung → `needsNotice` ohne Datei, Bestätigung merkt
    sich die Wahl nur für dieses Gerät, Kopie liegt unter `opened_encrypted`, Sweep entfernt sie.
    `files/streaming_tests.rs` (oder die vorhandene Testdatei) für `BytesSource`-Bereiche.
  - TypeScript: `scripts/check-passwords-format.ts` (vorhanden oder neu) – `fileKind` für Text, PDF,
    Bild, svg und „andere“.
  - E2E: `scripts/e2e/scenarios/passwords-attachments.test.ts` erweitern – Tipp auf einen
    Textanhang zeigt den Text, Escape gibt den Fokus zurück, Tipp auf das PDF zeigt den PDF-Viewer
    (das heutige „a tap on the PDF opens no lightbox“ bleibt wahr). „Mit System-App öffnen“ wird
    nicht im E2E geprüft (öffnet eine fremde App), sondern im Quickstart von Hand.
- **Rationale**: Tests in eigenen Dateien (Konstitution); gezielte `cargo test`-Ziele mit `-j 4`.
