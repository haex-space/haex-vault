# Vertrag: Bundle-Format `haextension-bundle/2`

Gilt für holzi (Prüfen) und das Werkzeug `haex` im vault-sdk (Erzeugen und Prüfen). Beide nutzen dieselbe
Umsetzung, das Rust-Crate `haex-bundle` im vault-sdk (holzi nativ, `haex` als WebAssembly). Begründung:
[research.md](../research.md) R2, R3. Die Testvektoren liegen im vault-sdk und werden mit Repository, voller
Revision und Pfad nach `src-tauri/tests/fixtures/extension_bundles/` kopiert (Constitution IV).

## Archiv

- Zip, Dateiendung `.xt`, höchstens 64 MiB.
- Einträge nur `Stored` oder `Deflate`, keine Verschlüsselung, keine Symlinks, keine Verzeichniseinträge mit
  Daten. Deterministisch erzeugt: feste Änderungszeit, keine Verzeichniseinträge.
- Eintragszahl im Archivende = Zahl der gelesenen Einträge (doppelte Namen sind sonst unsichtbar).
- Lokaler Kopf jedes Eintrags = sein Eintrag im zentralen Verzeichnis: Name, Methode, Verschlüsselung und, ohne
  Datendeskriptor, CRC-32 und beide Größen. Sonst sähe ein Leser, der dem lokalen Kopf glaubt, andere Daten.
- Höchstens 2.000 Einträge; je Eintrag ≤ 25 MiB entpackt und Verhältnis ≤ 200:1; gesamt ≤ 64 MiB entpackt.
  Ein Eintrag, der mehr Bytes liefert als angegeben, ist ein Fehler.
- Aufbau wie vom Schreiber erzeugt, damit jeder Zip-Leser dieselben Einträge sieht: Archivende in den letzten
  22 Bytes (kein Archivkommentar), Einträge lückenlos hintereinander ab Offset 0 in der Reihenfolge des
  zentralen Verzeichnisses, zentrales Verzeichnis direkt nach dem letzten Eintrag, keine Datendeskriptoren,
  keine Zusatzfelder, UTF-8-Kennzeichen bei Namen mit Nicht-ASCII-Zeichen. Ein Deflate-Strom endet genau am
  Ende der Eintragsdaten.

## Pfade

- Gültiges UTF-8 in NFC; nur `/` als Trenner; kein führendes `/`; kein `\`, `:`, NUL, Steuerzeichen.
- Keine leeren, `.`- oder `..`-Teile; Teil ≤ 255 Bytes; Pfad ≤ 1024 Bytes.
- Keine zwei Pfade, die nach NFC und Kleinschreibung gleich sind.

## Inhalt

| Pfad                                                                           | Pflicht          | Inhalt                                                     |
| ------------------------------------------------------------------------------ | ---------------- | ---------------------------------------------------------- |
| `haextension/manifest.json`                                                    | ja               | Manifest, byte-genau JCS (RFC 8785), ohne Feld `signature` |
| `haextension/signature.json`                                                   | ja               | Signaturdatei, JCS                                         |
| `<migrationsDir>/…`                                                            | wenn Migrationen | Drizzle-SQL und `meta/_journal.json`                       |
| alles andere                                                                   | –                | App-Dateien; Einstieg laut `entry` (Standard `index.html`) |
| `haextension.config.json`, `haextension/public.key`, `haextension/private.key` | **verboten**     |                                                            |

Eingeschränktes JSON in beiden Steuerdateien: nur ASCII-Schlüssel, nur ganze Zahlen im Bereich
−2^53 … 2^53, keine Gleitkommazahlen, keine doppelten Schlüssel. Nicht kanonische Bytes werden abgelehnt.

## `signature.json`

Hier lesbar umbrochen; die Datei selbst ist kompakt (JCS, ohne Leerzeichen).

```json
{
  "files": [
    { "path": "haextension/manifest.json", "sha256": "<64 hex>", "size": 1234 }
  ],
  "format": "haextension-bundle/2",
  "publicKey": "<64 hex>",
  "signature": "<128 hex>"
}
```

- `files`: jeder Eintrag des Archivs außer `signature.json`, sortiert nach UTF-8-Bytes des Pfads
  (JavaScript: `Buffer.compare`, nicht `.sort()`).
- `publicKey` = `manifest.publicKey`.
- **Signierte Nachricht**: `"haextension-bundle/2\n"` gefolgt von der JCS-Form von `signature.json` ohne den
  Schlüssel `signature`. Ed25519 (rein), Prüfung mit `verify_strict` und S < L, schwache Schlüssel abgelehnt.

## Block `openers` (Spec 063)

Ein Manifest darf einen Block `openers` führen, ein Array von Arten von Links, die die Extension öffnet
(heute `mail.compose`). Form, Grenzen und Prüfung stehen in
`specs/063-mailto-extensions/contracts/manifest-openers.md`; die Anmeldung gibt keinen Zugriff.

## Prüfung in holzi

Reihenfolge wie `haex verify` und die Testvektoren (`test-vectors/bundles/README.md`); die erste verletzte
Regel bestimmt die Fehlerart.

1. Archiv nach den Regeln oben lesen (nie `extract`): Größe, Archivende in den letzten 22 Bytes, zentrales
   Verzeichnis lesbar, Eintragszahl.
2. Erkennung des alten Formats, vor allen Eintragsregeln (alte Bundles enthalten verbotene Dateien und
   Verzeichniseinträge): Manifest mit Feld `signature` und keine `signature.json` → Fehler
   `legacy_signature_format` („mit dem aktuellen Werkzeug `haex` neu signieren“).
3. Jeder Eintrag in der Reihenfolge des zentralen Verzeichnisses: Art, Pfadregeln, Duplikate, Größen und
   Verhältnis, Aufbau (lückenlos, ohne Datendeskriptor und Zusatzfelder, UTF-8-Kennzeichen), lokaler Kopf,
   gelieferte Bytes, Ende des Deflate-Stroms und CRC-32; danach beginnt das zentrale Verzeichnis direkt nach
   dem letzten Eintrag. Die Aufbauregeln greifen erst hier, damit Bundles des alten Formats in Schritt 2
   erkannt werden.
4. Steuerdateien: Manifest vorhanden und kanonisch, `signature.json` vorhanden, kanonisch und wohlgeformt.
5. Für jeden Eintrag außer `signature.json` Pfad, Größe und SHA-256 berechnen; die Menge muss genau `files`
   entsprechen.
6. `manifest.publicKey == signature.publicKey`; Manifest gültig, sonst `manifest_invalid`:
   - `name` nach FR-004; `publicKey` gültiger Ed25519-Schlüssel, nicht von kleiner Ordnung;
   - `version` SemVer 2.0.0, wie das Rust-Crate `semver` sie liest (keine führenden Nullen außer in den
     Build-Metadaten, keine leeren Teile, Haupt-, Neben- und Patchnummer ≤ u64);
   - `entry` (Standard `index.html`) ist eine Datei des Bundles; `migrationsDir` ist ein gültiger Pfad;
   - `permissions` ist ein Objekt; seinen Inhalt liest [permissions.md](./permissions.md) (nicht angebotene
     Kategorien sind kein Fehler);
   - Migrationen: Mit `<migrationsDir>/meta/_journal.json` ist das Journal eingeschränktes JSON (nicht
     unbedingt kanonisch) mit einem Feld `entries`; jeder Eintrag hat ein eindeutiges ganzzahliges `idx` ≥ 0
     und ein eindeutiges `tag`, und `<migrationsDir>/<tag>.sql` ist eine Datei des Bundles. Ohne Journal sind
     die Migrationen die `*.sql`-Dateien direkt in `migrationsDir`. Jede Migrationsdatei ist UTF-8.
7. Signatur prüfen.

Fehlerarten (für Installation und Start): `archive_too_large`, `archive_invalid`, `entry_path_invalid`,
`entry_duplicate`, `entry_too_large`, `entry_ratio`, `entry_kind`, `manifest_not_canonical`,
`manifest_invalid`, `file_mismatch { path }`, `public_key_mismatch`, `signature_invalid`,
`legacy_signature_format`. Jede Art hat einen Test mit einem schlechten Bundle aus den Testvektoren, jede
Manifestregel aus Schritt 6 einen eigenen Vektor. `haex verify` und holzi müssen für jeden Vektor dieselbe
Fehlerart melden; eine Regel ohne Vektor gilt als nicht abgestimmt.

## Werkzeug `haex` (Änderung im vault-sdk, L0)

- `haex sign <dist>` schreibt v2: durchläuft mit `lstat` und bricht bei Symlinks ab, wendet die Pfadregeln an,
  schreibt das Manifest als JCS ohne `signature`, erzeugt `signature.json`, packt deterministisch.
- `haex verify <datei.xt>` prüft wie holzi.
- Schlüsselformat bleibt (öffentlicher Schlüssel roh als Hex), damit Präfixe der Tabellen gleich bleiben.
- Programmname einheitlich `haex` (heute `haexhub` in `src/cli/index.ts:13`).
- Gemeinsame Testvektoren: gute Bundles und je Regel ein schlechtes.
- Veröffentlichung als Breaking Change (Hauptversion 4).
