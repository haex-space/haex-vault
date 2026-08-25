# ADR 0002 — Shared-Space: Generischer Sync, Authentizität & Vertraulichkeit

- **Status:** Proposed (Design abgeschlossen — bereit für Implementierung ab Phase 0)
- **Date:** 2026-07-23
- **Deciders:** Martin Drechsel

Härtung des Shared-Space-Modells gegen einen nicht vertrauenswürdigen Leader, plus das
ursprünglich intendierte generische Teilen von Erweiterungs-Tabellen (Kalender, Chat, …)
und Dateien. Operativer Umsetzungs-Prompt (lokal): `docs/plans/2026-07-23-shared-space-phase0-pickup.md`.

---

## 0. Terminologie (verbindlich)

- **Vault-private Tabellen:** alle `haex_*`-System-Tabellen **außer den unten
  genannten Sync-Ausnahmen** (die 5 Infra-Tabellen + Share-Register) und explizit
  geprüften Register-Payloads. Kreuzen **nie** einen Space-Stream.
- **Space-Infrastruktur-Tabellen:** die 5 `SPACE_SCOPED_CRDT_TABLES`
  (`haex_space_devices`, `haex_space_members`, `haex_peer_shares`,
  `haex_mls_sync_keys`, `haex_device_mls_enrollments`). Die bewusste Ausnahme, die
  syncen **muss**, damit ein Space funktioniert. Bootstrap-Schicht.
- **Share-Register / Share-Eintrag:** `haex_shared_space_sync` + seine Einträge
  (Code: "assignments"). Ordnet eine Row einer Extension-Tabelle einem Space zu.
  **NICHT "Manifest"** — "Manifest" ist in diesem Projekt die Berechtigungs-Deklaration
  von Erweiterungen/External-Bridges (`acl-manifests.json`) und wird hier nicht gemeint.
  Das Register ist selbst **eine synchronisierte `haex_*`-Tabelle** (die **6.**
  Sync-Ausnahme, Bootstrap-Klasse neben den 5 Infra-Tabellen), **nicht** vault-private:
  sein Sync und seine Authentifizierung laufen über denselben Pfad wie die Infra-Tabellen
  — per-`(Spalte, space_id)`-Signatur (§4b), gekeyt per `space_id`. Sonst gäbe es keinen
  Kanal, über den ein register-getriebener Extension-Sync (§4a) überhaupt fließen könnte.
- **Nutzlast:** Erweiterungs-Daten + Datei-Inhalte — das eigentlich Geteilte.

Drei Schichten:
1. Vault-private `haex_*` → nie geteilt.
2. Space-Infrastruktur (die 5 Infra-Tabellen + Share-Register) → sync **1:1** via
   `space_id`-Spalte, signiert wie jede space-scoped Tabelle.
3. Erweiterungs-Daten + Dateien → **M:N**, getrieben durch das (synchronisierte)
   Share-Register (+ File-Sync). `haex_s3_backends` ist die einzige explizit geprüfte
   Systemtabellen-Ausnahme in dieser Schicht: geteilt wird ausschließlich die vom
   Remote-Storage-Flow erzeugte, gescopedte Child-Credential-Row.

---

## 1. Motivation

Es gibt heute **zwei** Sync-Transporte mit **ungleichem** Sicherheits-Reifegrad (§3/§3a),
und keiner erreicht das Schutzziel vollständig:

- Der **P2P-Pfad (Rust)** vertraut dem **Leader** implizit: er attestiert die Autorschaft
  von Zeilen (`authored_by_did`), setzt die materialisierte Capability durch und serviert
  allen Membern den Content im **Klartext**. In einem P2P-Space ist der Leader aber selbst
  ein Teilnehmer — ein böswilliger Leader kann Einträge **fälschen** (im Namen anderer) und
  den Content **mitlesen**.
- Der **server-vermittelte Pfad (TS)** ist bereits weiter — per-Spalte-Signaturen,
  Epoch-Key-Verschlüsselung, UCAN-Check beim Apply — aber **nicht leader-unabhängig**: die
  Autorisierung liest materialisierten DB-Zustand und winkt via Admin-Fallback jede
  self-issued Row durch, und die Signaturen sind nicht space-gebunden (§3a).

Diese ADR beschreibt daher nicht einen Neubau, sondern ein **einheitliches Zielmodell für
beide Pfade**: jeder Eintrag kryptografisch dem echten Autor zurechenbar, Autorisierung
leader-unabhängig gegen eine self-certifying Space-Root verifiziert, und Content
vertraulich.

Zusätzlich war das Share-Register als generischer Mechanismus gedacht, um **beliebige
Erweiterungs-Tabellen-Rows** (z.B. einen Kalendertermin, Multi-Space) in einen Space zu
teilen — dieser Mechanismus ist **nicht implementiert**. Das Zielmodell schließt ihn mit
ein, damit Chat, Kalender & Co. überhaupt syncen können.

---

## 2. Threat Model & Scope

### Schutzziel (Stufe 1 + Vertraulichkeit)

- **Authentizität/Integrität:** Wenn ich eine Spalte sehe, kann ich beweisen, dass DID X
  sie so geschrieben hat und niemand (auch kein Leader) den Inhalt verändert hat.
- **Autorisierung leader-unabhängig:** Nur Member mit echter Write-Capability (per
  signierter Delegations-Kette vom Space-Admin) dürfen schreiben — lokal verifizierbar.
- **Vertraulichkeit (forward-scoped):** Nur aktuelle Space-Member können **neu
  geschriebenen** Content entschlüsseln; Leader/Relay sehen nur Ciphertext.
  Grenze: Ein entfernter Member behält die **alten** Epoch-Keys und kann damit
  **historischen** Ciphertext (unter diesen Epochs verschlüsselt) weiter lesen —
  Schlüssel-Rotation allein macht Vergangenes nicht unlesbar. Rückwirkende
  Vertraulichkeit (Re-Encryption des retinierten Contents bei Membership-Wechsel)
  ist bewusst **out of scope** (siehe §4e).
- **Exfiltrationsresistenz:** kein Member kann Daten aus einem fremden Vault
  herausziehen (siehe Invariante §5).

### Bewusst akzeptiert (NICHT abgedeckt)

- **Rollback/Replay:** Leader darf eine ältere, korrekt signierte Version ausliefern.
- **Withholding/Zensur:** Leader darf Daten zurückhalten / nichts liefern.
- **Equivocation:** kein Schutz gegen "unterschiedlicher Zustand pro Empfänger".
- **Revocation-Propagation:** bleibt DB-/leader-getrieben (kollusiver Leader kann einen
  Tombstone zurückhalten → fällt unter Withholding). Signierte Revocation-Listen sind
  Folge-Arbeit.

### Grundsatz

Ein Member kann in einem Space beliebige Daten **in seinem eigenen Namen** erzeugen —
ok und erwartet. Verhindert wird nur das Erzeugen/Ändern **im Namen eines anderen**
sowie das Herausziehen fremder ungeteilter Daten.

---

## 3. Verifizierter Ist-Zustand (Audit 2026-07-23)

Es gibt **zwei** Sync-Transporte für Shared-Space-Daten, mit **unterschiedlichem**
Sicherheits-Reifegrad. Die folgende Tabelle auditiert den **P2P-Pfad (Rust,
`space_delivery/local`)** — den threat-model-relevanten Fall (Leader == Teilnehmer). Der
**server-vermittelte Pfad (TS, `src/stores/sync/orchestrator/pull|push`)** ist separat und
weiter — siehe §3a. Ziel dieser ADR ist u.a., beide zu **vereinheitlichen**, nicht alles
von Null zu bauen.

| Aspekt (P2P/Rust) | Ist-Zustand | Beleg |
|---|---|---|
| Was synct über P2P | Nur 5 **hartcodierte** Infra-Tabellen, gefiltert nach `space_id` | `crdt/scanner.rs:36-42`, `:315` "Tables outside … are never scanned" |
| Generischer Extension-Sync | **Nicht implementiert.** Scan-Satz konstant; Share-Register treibt keinen P2P-Sync | Register nur in `extension/spaces`, `remote_storage`, `database/create` |
| `haex_shared_space_sync` | Lokales **Register** ("Row → Space, Owner-Extension, Metadaten"); Payload liegt **nicht** darin | `extension/spaces/queries.rs:6-9` |
| Autorschaft | `authored_by_did` — vom **Leader** aus UCAN-Audience gesetzt; Leader-Attestierung, keine Autor-Signatur | `inbound_sync/validate.rs:52-87`, `leader/dispatch.rs:398-410` |
| CRDT-Merge | **Column-level** LWW mit per-Spalte-HLC → echte Spalten-Ko-Autorschaft | `crdt/scanner.rs` (`haex_column_hlcs`, `:225-260`) |
| Capability-Prüfung | Liest **materialisierten, leader-servierten DB-Zustand** (`haex_ucan_tokens.capability`, `haex_space_members`) | `space_delivery/local/ucan.rs:149-204` |
| UCAN-Kette | Ausstellung existiert (`create_delegated_ucan` mit `prf`, Root `space/admin`), aber `validate_token` **läuft `prf` nie** | `ucan/create.rs:74-103`, `ucan/verify.rs:140-156` |
| space_id-Bindung | **`crypto.randomUUID()`, NICHT an Creator-DID gebunden**; Root-UCAN self-signed | `src/stores/spaces/crud.ts:48,115`, `src/utils/auth/ucanStore.ts:28,45-47` |
| Content-Vertraulichkeit (CRDT) | **Klartext** ("Plain value (not encrypted)") | `crdt/scanner.rs:88`, `sync_loop/push.rs` |
| MLS-Content-Verschlüsselung | **Totes Gerüst.** `mls_encrypt`/`mls_decrypt` existieren, werden **nirgends** aufgerufen | `mls/manager.rs:257-286`, `lib.rs:785-786` |
| Was MLS heute tut | Nur Gruppen-/Epoch-Mgmt + Kill-Switch; Epoch-Keys existieren (`mls_export_epoch_key`), aber nicht zum Verschlüsseln genutzt | `sync_loop/mls.rs`, `mls/commands.rs` |
| File-Content-Verschlüsselung | **Nicht vorhanden** | `file_sync/` kein Encrypt-Pfad |

### 3a. Server-vermittelter Pfad (TS) — bereits weiter, aber lückenhaft

Der TS-Server-Sync (`orchestrator/pull/apply.ts`, `orchestrator/push.ts`; live via
`page.ts`/`cursor.ts`) implementiert **schon heute** einen Teil der Ziel-Mechanik — das
war im ursprünglichen Audit unterschlagen und relativiert drei P2P-Zeilen oben:

| Aspekt (TS/Server) | Ist-Zustand | Beleg |
|---|---|---|
| Autor-Signatur | **Vorhanden**, per-Spalte Ed25519 mit `signedBy` (Push signiert, Pull verifiziert Batch-atomar) | `apply.ts:70-98`, `push.ts:391-401` |
| Signatur-Preimage | `(tableName, rowPks, columnName, encryptedValue, hlcTimestamp)` — **signiert Ciphertext**, **ohne** `space_id`/`author_did` → nicht space-skopiert, encrypt-then-sign | `apply.ts:80-90`, `push.ts:391-400` |
| Content-Verschlüsselung | **Vorhanden**, Epoch-Key-basiert (`encryptedValue`/`nonce`/`epoch`, `mls_get_epoch_key`, `decryptCrdtData`) | `apply.ts:296-355` |
| UCAN-Autorisierung beim Apply | **Vorhanden aber leader-/DB-abhängig**: liest materialisierte `haex_ucan_tokens`; `validateUcan` prüft **ein** Token, **nicht** die `prf`-Kette zur Space-Root | `apply.ts:100-135` |
| Admin-Fallback | **Schwachstelle**: autorisiert **jede** self-issued Row (`issuer == audience == signer`) allein durch Existenz — keinerlei Bindung an eine self-certifying Space-Root | `apply.ts:137-150` |

Konsequenz: Der server-vermittelte Pfad erfüllt Authentizität/Vertraulichkeit teilweise,
ist aber **im Threat-Model-Sinn noch nicht leader-unabhängig** (Autorisierung vertraut
DB-Zustand + Admin-Fallback) und die Signaturen sind nicht space-gebunden. Die Ziel-
Architektur (§4) und die Phasen (§7) adressieren **beide** Pfade explizit.

---

## 4. Ziel-Architektur

Fünf Komponenten. Getrennt entwerfbar, greifen aber ineinander.

### 4a. Generischer Erweiterungs-Sync (Share-Register, Multi-Space)

- Eine Row (z.B. Kalendertermin) muss in **mehrere** Spaces teilbar sein (M:N) →
  `haex_shared_space_sync` wird die **synchronisierte, signierte Zuordnungs-Ebene**.
- Ein **Share-Eintrag** = signierte Behauptung: *"Member A teilt Row R (Tabelle T,
  PK P) in Space S"* (+ Zugriffsart, siehe 4d). Signiert vom **Sharer**.
- **Sync wird register-getrieben** (nicht `space_id`-Spalten-getrieben) für
  Extension-Tabellen: der Scanner sammelt pro Space die eigenen Share-Einträge und
  scannt die referenzierten Content-Rows.
- **Content-Row-Identität:** UUID-PK → kollisionsfreies Landen in der gleichnamigen
  Tabelle des Empfängers. Fehlt die Extension/Tabelle → **store-and-ignore**.
- **Relay ist gewollt:** Sharer ≠ Content-Autor ist erlaubt (B relayed A's Daten). Die
  Autorschaft bleibt via per-Spalte-Signatur (4b) path-unabhängig sichtbar — man sieht,
  dass es A's Daten sind, auch wenn man sie über B bekam. Ein Content-Hash im Share-Eintrag
  ist **nicht** nötig (Content ist über seine Spalten-Signaturen selbst-authentifizierend).

### 4b. Authentizität — per-(Spalte, space_id) Autor-Signatur

**Granularität = Merge-Granularität = column-level, uniform für ALLE space-scoped
Tabellen** (inkl. der 5 Infra-Tabellen). Signiert man per Row aber merged per Spalte,
ist die Row nach gemischtem Merge inkohärent.

- **Signiert wird** (kanonisch encodiert):
  `(space_id, table_name, row_pks, column_name, hlc, author_did, value)`.
  Da `space_id` + `author_did` im Preimage sind, ist eine Signatur **intrinsisch auf
  (Space, Identität) skopiert.**
- **Kanonische Serialisierung (§6.2 gelöst):** KEINE JSON-Kanonisierung. Domain-separierte,
  längen-präfixierte Konkatenation:
  `domain_tag ‖ len‖space_id ‖ len‖table ‖ len‖row_pks ‖ len‖column ‖ len‖hlc ‖ len‖author_did ‖ len‖value_bytes`,
  dann ed25519. **`value_bytes` = exakt die kanonische *Klartext*-Byte-Form des Wertes**
  (die Speicher-Repräsentation **vor** der Verschlüsselung), nie re-serialisiertes JSON
  und **nie der übertragene Ciphertext**. `domain_tag` = fester Präfix
  `"haex/space-col-sig/v1"` gegen Cross-Protocol-Reuse. ⚠️ Impl-Risiko: Value darf durch
  die Pipeline nie re-serialisiert werden (`1.0`→`1`); signierte Bytes verbatim mitführen.
- **Storage-Class-Tag in `value_bytes` (Phase-1-Review-Nachtrag):**
  `value_bytes = storage_class_tag (1 Byte) ‖ native Byte-Form`, Tag-Werte = SQLites eigene
  Typ-Codes (`INTEGER=1, REAL=2, TEXT=3, BLOB=4, NULL=5`). Die Längen-Präfixe trennen
  **Felder**, nicht **Typen innerhalb eines Feldes** — ohne Tag kodieren `NULL`, `TEXT('')`
  und `BLOB([])` alle zur leeren Byte-Folge, und `Integer(1)` kollidiert mit
  `Blob([0,0,0,0,0,0,0,1])`. Ein Angreifer könnte also eine gültige Signatur über einen
  Wert replayen und dabei einen byte-gleichen Wert einer anderen Storage-Class einsetzen
  (`NULL` → `""` ist der einfache Fall). Der Tag macht jede Storage-Class zu einem eigenen
  Preimage. Implementiert in `value_bytes.rs::tag`, `columnSigCanonical.ts::STORAGE_CLASS_TAG`
  und `scripts/gen-column-sig-vectors.ts`; Fixture-Vektoren `empty_text_valid` /
  `empty_blob_valid` / `null_class_probe_valid` sichern es cross-language ab.
- **Signatur-über-Klartext (kohärent mit 4e):** Die Signatur deckt den **Klartext**
  (`value_bytes`), transportiert/gespeichert wird der Ciphertext (**sign-then-encrypt**).
  ⚠️ Der heutige TS-Server-Sync macht das **Gegenteil** (signiert `encryptedValue` =
  Ciphertext, encrypt-then-sign) **und** lässt `space_id` + `author_did` aus dem Preimage
  (`push.ts`/`apply.ts`, siehe §3) → Signaturen sind heute **nicht** space-skopiert und
  space-übergreifend re-spielbar. Phase 1 stellt beides um: Klartext signieren, `space_id`
  + `author_did` ins Preimage.
- **Storage:** neue CRDT-Metadaten-Spalte `haex_column_sigs`, gekeyt **pro (Spalte,
  space_id)**: `column -> { space_id -> {authorDid, sig, storageClass} }`. Parallel zu
  `haex_column_hlcs`. `storageClass` erhält die von JSON/IPC verlorene Unterscheidung
  zwischen INTEGER/REAL/TEXT/BLOB/NULL. Infra-Tabellen = degenerierter Fall (genau ein
  Space).
- **`authored_by_did` wird KOMPLETT gelöscht** (§6.3 gelöst): bei Ko-Autorschaft
  row-level bedeutungslos. Autoritative Autorschaft ist die per-Spalte `author_did`.
  Die zwei FK-Parent-Trigger (`*_ensure_refs`) lasen `NEW.authored_by_did` → die
  **Stub-Erzeugung (haex_identities/haex_devices) zieht nach Rust** in den Apply-Pfad,
  wo die verifizierte `author_did` ohnehin bekannt ist.
- **Verifier & Reihenfolge:** jeder Apply-Pfad verifiziert **vor** dem Merge gegen den
  aus `author_did` abgeleiteten Public Key. Bei verschlüsseltem Content ist die
  Reihenfolge **decrypt → verify → merge** (der Verifier braucht den Klartext
  `value_bytes`, denn die Signatur deckt den Klartext). Choke-Points sind **beide**
  Apply-Pfade: Rust `apply_remote_changes_to_db` (P2P) **und** der TS-Server-Sync
  (`verifyPulledChangesAsync` → `applyRemoteChangesInTransactionAsync`, `apply.ts`, siehe
  §3/§4c). Ungültig/fehlend → Change verworfen (Batch als Ganzes).

### 4c. Autorisierung — UCAN-Delegations-Kette leader-unabhängig verifizieren (Option 2)

Infrastruktur existiert (Ausstellung mit `prf`, Root-`space/admin`-UCAN). Fehlt: **Ketten-
Verifikation zur Laufzeit, verankert in der Space-Root.** Drei Bausteine:

1. **Space-Identität self-certifying machen (Phase 0, siehe §6.1):**
   `space_id = base58btc(nonce ‖ sha256_trunc16(domain_tag ‖ nonce ‖ root_did))` — die
   16-Byte-Nonce ist **Präfix** der id (self-contained; kein separater Nonce-Transport
   nötig, kein `nonce`-Fact im Root-UCAN). Verifikation: Nonce aus der id splitten,
   `root_did` aus der Root-UCAN-Issuer-DID, Hash neu berechnen und mit dem Hash-Teil der
   id vergleichen. Match → die Wurzel gehört zu genau dieser id; Root-UCAN sonst ungültig.
   Verify-Signatur ist damit `(space_id, root_did) → bool` — keine Seitenkanal-Daten
   nötig. `len‖`-Prefixes sind bei fixed-16-Byte-Nonce + trailing DID **nicht**
   erforderlich (SHA-256-Padding kodiert die Input-Länge intern → Preimage unambiguous).
2. **`prf`-Kette bei der Verifikation laufen:** Member-Token ← Admin-Delegation ←
   Root-`space/admin`; jede Signatur + Capability-Attenuierung geprüft, Wurzel == gebundene
   Space-Root. Ersetzt das Vertrauen in die `capability`-DB-Spalte. **Gilt für _jeden_
   Autorisierungs-Entscheid auf _jedem_ Apply-Pfad** — insbesondere den TS-Pfad
   `verifyPulledChangesAsync` (`apply.ts`), der heute materialisierte `haex_ucan_tokens`
   liest und via Admin-Fallback jede `issuer == audience`-Row durchwinkt (§3a). Dieser
   Fallback **entfällt**: auch der Root-`space/admin` muss über seine Bindung an die
   self-certifying `space_id` (Baustein 1) verifiziert werden, nicht über eine
   self-issued DB-Row.
3. **Kette reist mit / ist unabhängig verifizierbar** (Tokens liegen in
   `haex_ucan_tokens`) — Empfänger re-verifiziert selbst.

Zusammenspiel mit 4b: Signatur beweist *wer*; Kette beweist, dass dieser *wer*
Write-Autorität hatte. Beides lokal, ohne Leader.

### 4d. Ressourcen-Zugriffsarten (read-only / writable), objekt-zentriert

**Komposition mit Member-Capability (nicht Ersetzung):**
- **Member-Capability bleibt Ceiling:** Admin lädt ein, vergibt read / write / invite
  (deckt den Konferenz-/Klassen-Space ab, in dem nur der Admin schreibt).
- **Zusätzlich pro geteiltem Item** (Row bzw. Datei/Ordner) eine vom **Owner signierte**
  Zugriffsart (read-only / writable), Teil des Share-Eintrags.
- Write gültig nur, wenn **beides** erlaubt: Member hat Space-Write **UND** Ressource ist
  writable (bzw. Schreiber == Owner).
- Signierte Zugriffsart ist **lokal verifizierbar** ("Änderung an R von M; R vom Owner A
  als read-only signiert; M ≠ A → verwerfen") ohne Leader.

### 4e. Vertraulichkeit — MLS-Epoch-Key-Content-Verschlüsselung

MLS-Fundament steht; das Anschließen fehlt. **sign-then-encrypt: signiert wird der
Klartext (`value_bytes`, §4b), übertragen/gespeichert der Ciphertext.** Verify-Order beim
Apply daher **decrypt → verify → merge** (§4b). Retentionsregel für die Verifikation: die
signierten Klartext-`value_bytes` werden **nicht** persistiert; verifiziert wird transient
nach dem Entschlüsseln, bevor gemergt wird.

- **CRDT-Payload (server-vermittelt, Modus b):** `value` mit dem **Space-Epoch-Key
  sealen** beim Push, öffnen beim Apply. Der Sync-Server als Relay sieht nur Ciphertext.
  ✅ **SHIPPED.** `push.ts` wählt den Key (`mls_export_epoch_key` für Shared Spaces,
  Vault-Key für den eigenen Vault), `tableScanner.ts` sealt jeden Spaltenwert, auf dem
  Wire liegen nur `encryptedValue` + `nonce` + `epoch`. Der Empfänger holt in `apply.ts`
  den Key zur mitgereisten `epoch` und entschlüsselt vor dem Verify (§4b).
- **CRDT-Payload (P2P, Modus c):** Content bleibt **Klartext**; die Vertraulichkeit
  hängt an der QUIC-Transportverschlüsselung (`crdt/scanner.rs`,
  `LocalColumnChange.value`). Anders als der Sync-Server ist die Gegenstelle hier ein
  Space-Member, das den Epoch-Key ohnehin besitzt — eine Content-Verschlüsselung mit
  genau diesem Key würde gegen sie nichts schützen. **Offen und nicht entschieden:** der
  P2P-Leader relayed Changes zwischen Peers; ob er dabei ausschließlich Content sieht,
  für den er selbst Leseberechtigung hat, ist im Zusammenspiel mit den per-Ressource-
  Zugriffsarten (§4d) nicht nachgewiesen. Erst wenn dort eine Lücke belegt ist, wird
  P2P-Content-Verschlüsselung sinnvoll — und dann **nicht** mit dem Epoch-Key.
- **Multi-Space:** eine Row in mehreren Spaces → an **jede** Space-Gruppe separat
  verschlüsseln.
- **Dateien (Cloud-Ziele):** File-Content mit dem Epoch-Key verschlüsseln, im
  `file_sync`-Engine am `cloud_provider`-Pfad. Der Storage-Betreiber ist ein Dritter
  ohne Key; heute liegt dort alles im Klartext. Verbindliche Form:
  - **Opake Object-Keys.** Der Objektname ist eine Zufalls-ID, nicht der relative Pfad.
    Andernfalls lernt der Betreiber Dateinamen und Ordnerstruktur — die bei einem Vault
    regelmäßig so sensibel sind wie der Inhalt selbst.
  - **Metadata-Sidecar.** Name, Größe, Typ, mtime und Klartext-Hash liegen in einem
    kleinen Begleitobjekt `<key>.m`, verschlüsselt wie der Content. Member listen einen
    Bucket, indem sie nur die Sidecars laden — nicht die Inhalte. Die Zuordnung
    Pfad→Object-Key wird lokal in `haex_sync_state_no_sync` gecached; ein frisches Gerät
    baut den Cache einmalig aus den Sidecars auf.
  - **Selbsttragender Bucket.** Bucket + Epoch-Keys genügen für ein vollständiges
    Restore inklusive Dateinamen. Die Metadaten liegen deshalb bewusst **nicht** nur in
    einer CRDT-Tabelle: ein Backup, das ohne zweites synchronisiertes Artefakt nicht
    wiederherstellbar ist, wäre kein Backup.
  - **Envelope.** Jedes Objekt beginnt mit Magic + Version + `epoch` + File-Nonce,
    danach Chunks mit je eigenem AEAD-Tag (Chunk-Nonce aus File-Nonce + Chunk-Index).
    Selbstbeschreibend, damit der Leser den Key ohne Backend-Metadaten wählen kann;
    chunk-weise, damit Resume und Range-Download erhalten bleiben.
  - **Verbleibender Leak (akzeptiert):** Objektanzahl, Objektgrößen und
    Änderungsfrequenz. Größen zu verschleiern bräuchte Padding — eigener Scope.
- **Dateien (P2P-Ziele):** bleiben **Klartext**. `PeerProvider` öffnet einen direkten
  QUIC-Stream zum Endpoint des Empfängers und autorisiert jeden Request per UCAN; ein
  Leader ist am Datei-Transfer nicht beteiligt, und das `relay_url` ist der
  iroh-NAT-Traversal-Relay, der QUIC nicht terminiert. Empfänger ist also genau der,
  der die Datei bekommen soll, und er darf sie lesen — Verschlüsselung mit einem Key,
  den er selbst hält, schützt gegen niemanden.
- **Key-Rotation:** Membership-Änderung → neue Epoch; entfernte Member verlieren
  Lesezugriff auf **neue** Epochs. Sie behalten die alten Epoch-Keys und können
  damit weiterhin **historischen** Ciphertext entschlüsseln (siehe Invariante §2,
  "forward-scoped"). Rückwirkende Vertraulichkeit bräuchte eine Re-Encryption des
  retinierten Contents unter der neuen Epoch — **out of scope** dieser Phase
  (Folge-Arbeit, §8).
- **Historische Epochs für neue Member — bewusste Abweichung von MLS.**
  `haex_mls_sync_keys` hält **eine Zeile pro Epoch** (`space_id, epoch, key_data`) und
  ist CRDT-gesynct; Leser holen den Key zur Epoch, die am Ciphertext steht. Folge: ein
  **neu beitretendes** Member erhält über den Tabellen-Sync auch die Keys der Epochs
  **vor** seinem Beitritt und kann damit den bestehenden Space-Inhalt lesen. MLS selbst
  würde das verhindern — ein Joiner bekommt dort keine historischen Group-Secrets. Die
  Abweichung ist **gewollt**: ein Member, das einem Space beitritt, soll dessen
  vorhandene Daten und Dateien sehen, sonst wäre geteilter Content für Nachzügler
  wertlos. Konsequenz, die dabei in Kauf genommen wird: Leserechte sind nicht auf den
  Zeitraum der Mitgliedschaft eingrenzbar. Wer das braucht, bräuchte pro Epoch eine
  Zugriffsentscheidung statt eines gesyncten Key-Sets — nicht Teil dieses ADR.
- **Consent-Nuance (Relay):** B kann A's Daten nur in einen neuen Space relayen, wenn B
  entschlüsseln *und* für die neue Gruppe neu verschlüsseln kann. A hat dem neuen Space
  nicht zwingend zugestimmt — reine Vertraulichkeits-/Zustimmungsfrage dieser Phase.

---

## 5. Sicherheits-Invarianten (benannt, nicht verhandelbar)

### I1 — `haex_*`-Ausschluss
Das Share-Register darf **nie** eine vault-private `haex_*`-Tabelle als Ziel akzeptieren
— **inklusive sich selbst** (`haex_shared_space_sync` ist kein zulässiges Share-Ziel).
Die Zugangs-Prüfung **komponiert**: Extension-Tabellen via Register erlaubt; die 5
Infra-Tabellen **und das Share-Register selbst** syncen via `space_id` (Bootstrap-Pfad,
nicht über einen Register-Eintrag); **alle nicht explizit geprüften `haex_*`
kategorisch verboten auf beiden Pfaden.** Die zusammengesetzte Prüfung besteht aus
`is_space_scoped_table()` für die 6 Bootstrap-Ausnahmen und
`is_register_target_forbidden()` für Register-Payloads. Als Register-Ziel ist zusätzlich
nur `haex_s3_backends` erlaubt; jede neue System-Tabelle bleibt ohne explizite
Security-Review ausgeschlossen.

### I2 — Owner-initiiertes Teilen / Exfiltrationsresistenz
Ein Vault sendet **nur Daten, die physisch in seiner eigenen DB liegen**, und bestimmt
"was ich teile" **ausschließlich aus Share-Einträgen, die er selbst signiert hat**.
Fremd-signierte Register-Einträge lösen **niemals** einen Push eigener Daten aus. Ein
Share-Eintrag ist eine **Push-Deklaration des Owners, keine Pull-Anforderung**.

> Konkreter Guard: Der Register-Lookup im Push-/Signier-Pfad
> (`core::execute_with_crdt`) filtert `referenzierte Row gehört mir UND Eintrag ist von
> mir signiert`. So kann ein via Sync in die eigene DB gelangter, fremd-signierter Eintrag
> nie ein Self-Exfiltration auslösen.

Defense-in-Depth: signierte Einträge (unfälschbar "wer teilt") + leader-seitiger
`filter_ownership_violations` (`inbound_sync/mod.rs:44-49`).

---

## 6. Signier-Punkt, Key & gelöste Detailfragen

### Signier-Punkt & Key
- **Wo:** im Core-Write-Pfad **`core::execute_with_crdt`** (der Chokepoint für alle SQL
  auf Sync-Tabellen). Extensions bekommen **nie** den Identity-Private-Key.
- **Welcher Key:** der Private Key der **Space-Member-Identität**, unter der der Vault dem
  Space beigetreten ist (`SQL_SELECT_OWN_DID_FOR_SPACE`).
- **Multi-Space + Multi-Identity:** eine geteilte Row hat **ein Signatur-Set pro Space**,
  je unter der Identität dieses Spaces. `execute_with_crdt` wird **register- und
  identity-aware**. Signier-Zeitpunkte (beide durch `execute_with_crdt`):
  1. **Spalten-Write:** signiere für alle Spaces, in denen die Row gerade ist (eigene
     Share-Einträge, siehe I2) — je Space die Identität auflösen.
  2. **Neuer Share-Eintrag:** signiere alle aktuellen Spalten der Row für die Identität
     des neuen Spaces (Cross-Table-Seiteneffekt).

### §6.1 space_id-Bindung — GELÖST
Fehlt heute (Zufalls-UUID). Fix: self-certifying
`space_id = base58btc(nonce ‖ sha256_trunc16(domain_tag ‖ nonce ‖ root_did))` bei
Erstellung (`createLocalSpace`/`createOnlineSpace`). Die 16-Byte-Nonce ist **Präfix**
der id (self-contained: kein separater Nonce-Transport nötig, kein UCAN-Fact). Domain-Tag
ist ein fester 16-Byte-String `"haex/space-id/v1"` gegen Cross-Protocol-Reuse; Hash
wird auf 16 Byte gekürzt (128-Bit Second-Preimage-Sicherheit). Encoding: base58btc
(Bitcoin-Alphabet, ~44 Zeichen). Verifier splittet die Nonce aus der id, rechnet den
Hash neu und vergleicht — Verify-Signatur ist `(space_id, root_did) → bool`. Keine
`len‖`-Prefixes nötig (fixed-16-Byte-Nonce + trailing DID + SHA-256-internes Padding
= unambiguous). Keine Migration (keine Prod-Nutzer).

### §6.2 Kanonisierung — GELÖST
Siehe 4b (Domain-Tag + gespeicherte value_bytes).

### §6.3 `authored_by_did` — GELÖST
Komplett löschen, Stub-Erzeugung nach Rust (siehe 4b).

### §6.4 Content-Bindung — GELÖST
Kein Content-Hash nötig; Relay via Sharer≠Autor (siehe 4a).

### §6.5 Delete-Handling — REVIDIERT 2026-07-29

**Grundsatz:** Deletes/Unshares signieren (gleiche Maschinerie, Phase-1 Column-Sig).
Zugriffsart = Write (kein separates Delete-Cap — Write-Member könnte via Overwrite
inhaltlich ohnehin löschen). Ein weggelassener Eintrag = akzeptiert (Withholding);
ein injizierter gefälschter Delete im Namen von A = aktive Manipulation → ausgeschlossen.

**Zwei Delete-Log-Domänen.** Frühere Formulierung *"Deletes laufen über
`haex_deleted_rows`"* war pre-Shared-Space und stale — `haex_deleted_rows` ist NICHT in
der Shared-Space-Whitelist (`SPACE_SCOPED_CRDT_TABLES`) und erreicht Space-Peers nie:

- **Owner-Domain** — `haex_deleted_rows` (bestehend). Fließt ausschließlich zwischen
  den Geräten desselben Owners via `scan_all_crdt_tables_for_owner`.
- **Shared-Space-Domain** — `haex_shared_space_deleted_rows` (neu, per-Space,
  `space_id`-Column). Steht in der Shared-Space-Whitelist. Fließt an Members
  über alle drei Sync-Modi (Server-Relay, P2P, Federation).

**Signal-Ableitung.** `BEFORE DELETE`-Trigger auf einer Business-Tabelle cascadiert
lokal in Register-Cleanup (`DELETE FROM haex_shared_space_sync WHERE ...`) + schreibt
per-Space-Einträge in `haex_shared_space_deleted_rows`. Unshare (Register-DELETE ohne
Row-Löschung) erzeugt denselben Peer-Signal-Typ. Empfänger führt Business-Row-DELETE
UND Register-Cleanup aus **einem einzigen** Signal-Row aus — Register-Cleanup wird lokal
abgeleitet, keine zwei Signale nötig.

**Apply-Gate (Register-Check).** Vor Ausführung des lokalen Business-Row-DELETEs prüft
der Empfänger `(target_table, target_row_pks, target_space_id)` gegen das Register.
Nur bei **positiver Register-Evidenz** für den claimed Space werden Register-Cleanup +
Business-Row-DELETE ausgeführt. Fehlt der Register-Eintrag lokal, ist der Apply ein
row-scoped No-op — zwei Sub-Fälle:

- **In anderem Space registriert** (`any_space_registered = true`) → suspected forgery
  (`NotSharedInSpace`). Business-Row bleibt intakt; das Register-Entry des anderen
  Spaces wird nicht angefasst.
- **Nirgends registriert** → Race mit lokalem Unshare (Sender-Seite hat Register-DELETE
  bereits gefanned out; Empfänger applyt das Signal auf einen bereits unshareten Zustand).
  Unshare hält die Business-Row per §6.5-Grundsatz — kein Business-DELETE.

Datenbank-Fehler auf einem der Register-Lookups sind fail-closed: Log + skip des
Signal-Eintrags, damit ein transienter DB-Fehler nie stillschweigend einen Delete
autorisieren kann.

**Compaction-Anchor.** Pro Space in `haex_space_compaction_anchors` (synced,
max-wins-merge, Leader-only advance). Retention-Job pruned Delete-Log-Einträge älter
als `N` Tage UND schiebt den Anchor synchron nach vorne. Push-Batches mit
`haex_hlc < anchor` → Reject; Peer muss Refresh-Pull. Verhindert Zombie-Wieder-
auferstehung nach Retention-Pruning. Owner-Domain nutzt denselben Mechanismus mit
einem globalen Anchor in `haex_vault_settings` (statt per-Space).

**Race-Handling (idempotent).** Wenn ein Delete-Log-Signal beim Apply auf einen
lokal bereits gelöschten Target-Row trifft (paralleles Unshare + Hard-Delete oder
Multi-Hop-Sync-Ordnung): No-op statt Reject. Konvergenz > exakte Ordnung; passt zum
"weggelassener Eintrag = akzeptiert" aus §2.

**Cross-Space-Relay (bewusst nicht abgedeckt, §2-scope):** Bob kann von Alice erhaltene
Rows explizit weiter-sharen (Alice → Bob in X → Charlie in Y). ADR §2 listet
Relay-Consent als "bewusst akzeptiert / nicht abgedeckt"; Härtungs-Aspekt in Phase 4.

**Follow-up-Tickets** (nicht Teil dieses Abschnitts):
- **HLC-Sanity-Check system-weit** — Poisoning-Schutz für alle received HLCs
  (`remote_hlc.wall_time <= local_wall_time + MAX_SKEW`), gilt für jede CRDT-Row nicht
  nur Deletes.
- **Rust ↔ TS Shared-Space-Whitelist-Alignment** — Rust `SPACE_SCOPED_CRDT_TABLES` als
  single source of truth, TS konsumiert per ts-rs-Binding statt zu duplizieren.

### §6.6 Performance
Keine Design-Entscheidung — Impl-Validierung (per-Spalte ed25519 auf Bulk messen).

---

## 7. Phasierung

Keine Backward-Compat nötig (keine Prod-Nutzer) → sauberer, breaking Umbau erlaubt.
Reihenfolge so, dass Sicherheits-Invarianten nie regredieren (Signatur vor generischem
Sync, damit Extension-Daten nie ungeschützt fließen).

- **Phase 0 — Self-certifying space_id.**
  `space_id = base58btc(nonce ‖ sha256_trunc16(domain_tag ‖ nonce ‖ root_did))`;
  16-Byte-Nonce als Präfix in der id (self-contained). Verifier rechnet nach und prüft
  Bindung. *Fundament für Option 2; ohne dies kein vertrauenswürdiger Anker.*
- **Phase 1 — Per-(Spalte, space_id) Autor-Signatur (4b)** auf allen space-scoped Tabellen
  (die 5 Infra-Tabellen + Share-Register zuerst): `haex_column_sigs`, Signieren in
  `execute_with_crdt`, Verifizieren im Apply, `authored_by_did` löschen + Stubs nach Rust.
  Schließt die **Umstellung des bestehenden TS-Preimage** ein (§3a): `space_id` +
  `author_did` aufnehmen und Klartext statt Ciphertext signieren (sign-then-encrypt).
  ✅ **SHIPPED via PR #718 (2026-07-28).** `authored_by_did` wurde nur aus
  `haex_shared_space_sync` (Migration 0012) gedroppt; die 5 SPACE_SCOPED_CRDT_TABLES
  (`haex_space_devices`, `haex_space_members`, `haex_peer_shares`, `haex_mls_sync_keys`,
  `haex_device_mls_enrollments`) behalten die Spalte bewusst — die Leader-Injection in
  `inbound_sync/validate.rs` bleibt dort die einzige Anti-Forgery-Maßnahme bis zu einer
  späteren Runde. Vertraulichkeit: `value_bytes` liegt nie auf dem Wire (Ship-Blocker
  aus Runde 7); Empfänger canonicalisiert den entschlüsselten Wert lokal via
  `toCanonicalBase64` und batched dann zu `verify_column_sig_batch` (Rust-Command).
  ✅ **Review-Follow-up:** Der P2P-Pfad transportiert und erzwingt die Signatur nun
  ebenfalls, persistiert verifizierte Signaturen für Relay, synchronisiert das
  Share-Register als sechste Bootstrap-Tabelle und verwirft unsignierte
  Shared-Space-Changes. Das Register treibt einen Push nur bei selbst signierten
  Routing-Spalten (I2); neue `haex_*`-Ziele fallen geschlossen aus (I1).
- **Phase 2 — UCAN-Delegations-Ketten-Verifikation (4c):** `prf` laufen, Root-Anker — auf
  **beiden** Apply-Pfaden. Für den TS-Pfad (`verifyPulledChangesAsync`, `apply.ts`): volle
  `prf`-Kette statt materialisiertem `haex_ucan_tokens`, und den Admin-Fallback
  (`issuer == audience`) entfernen (§3a/§4c). ✅ **Implementation delivered PR #717
  (2026-07-24); 2-Geräte-Validierung via echten P2P-E2E-Test abgeschlossen
  (2026-08-03, `haex-e2e-tests: write-capability-p2p-enforcement.spec.ts`):**
  Member ohne Write-Cap → Write vom Leader abgelehnt; Member mit Write-Cap →
  Write akzeptiert. Dabei Regression gefunden + gefixt: Invite-Claim nahm per
  `.next()` nur das erste Element aus dem Capabilities-Array, wodurch jede
  Mehrfach-Einladung (z.B. `["space/read","space/write"]`) beim Claim auf
  eine einzelne Capability kollabierte. Capabilities sind orthogonal, keine
  Rangfolge (ein Member kann Write UND Invite halten, ohne dass eines das
  andere impliziert) — der Fix erstellt daher **ein UCAN pro Capability**
  statt eine "höchstrangige" auszuwählen (`invite_tokens.rs`,
  `Response::InviteClaimed.granted: Vec<ClaimedCapabilityUcan>`).
  Zusatz: einheitliche Verifier-Implementation in Rust; TS ruft `verify_ucan_chain_batch`
  Tauri-Command. Row-scoped Rejection + aggregierter User-Toast statt Batch-Abbruch.
- **Phase 3 — Generischer register-getriebener Extension-Sync (4a)** + signierte
  Ressourcen-Zugriffsarten (4d) + Invarianten I1/I2. Chat & Kalender werden hier möglich.
- **Phase 3.a — Shared-Space Delete-Propagation (V2).** Prerequisite/parallel für Phase 3.
  Impl: `haex_shared_space_deleted_rows` (per-Space Delete-Log),
  `haex_space_compaction_anchors` (Anti-Resurrection), Register-DELETE-Cascade,
  Register-Check-Gate, Retention + Anchor-Advance im Leader. Design-Details in §6.5
  (revidiert 2026-07-29). Impl-Plan liegt lokal unter
  `docs/plans/2026-07-29-shared-space-delete-propagation.md`.
- **Phase 4 — Vertraulichkeit (4e).** Reihenfolge **korrigiert 2026-08-25** (vorher:
  "CRDT-Payload-Verschlüsselung, dann File-Content"): die CRDT-Payload-Verschlüsselung
  gegen den server-vermittelten Relay ist bereits mit Phase 1 gelandet (§4e), und der
  P2P-Pfad braucht sie nicht, weil die Gegenstelle den Epoch-Key ohnehin hält. Es
  verbleibt daher **nur der Datei-Pfad, und dort nur die Cloud-Ziele**: Content-
  Verschlüsselung mit dem Epoch-Key, opake Object-Keys, Metadata-Sidecars und
  Chunk-Envelope gemäß §4e. P2P-Datei-Transfer bleibt Klartext (Begründung in §4e).

Jede Phase ist für sich testbar und liefert Wert.

---

## 8. Explizit außerhalb des Scopes (Folge-Projekte)

- Rollback-/Replay-Schutz (per-Autor-Hash-Kette).
- Equivocation-Schutz (verifiable log pro Space).
- Leader-unabhängige Revocation (signierte Revocation-Listen).
- Relay-Consent (A's Zustimmung, wenn B in neuen Space relayed) — Teil-Aspekt von Phase 4.
- Rückwirkende Vertraulichkeit / Re-Encryption des retinierten Contents bei
  Membership-Wechsel (aktuell nur forward-scoped, siehe §2/§4e).

---

## 9. Offene Impl-Detailfragen (in der jeweiligen Phase zu klären)

- Genaues Encoding der self-certifying `space_id` (Hash-Funktion, nonce-Länge, base58 vs.
  base32), **wo genau die `nonce` mitreist** (Root-UCAN-Fact vs. Space-Metadaten) + wo der
  Verifier die Bindung prüft (Phase 0).
- Byte-Stabilität von `value_bytes` durch die gesamte Pipeline (Phase 1).
- Wie `execute_with_crdt` effizient "meine Spaces für diese Row" auflöst, ohne pro Write
  teure Register-Scans (Phase 1/3).
- Schlüssel-Handling bei Multi-Identity-Signierung (mehrere Private Keys pro Write laden).
