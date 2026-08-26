# Glossary

Canonical definitions of terms used across this project. One line per term.
Design decisions live in `docs/adr/`; this file is a glossary only.

## Sync-Modi

- **Sync-Modus (a) — Personal Vault Sync** — Owner ↔ eigener Sync-Server, voller CRDT-Table-Set, keine Whitelist-Restriktion.
- **Sync-Modus (b) — Server-vermittelter Shared-Space** — Sync-Server als Relay für Shared-Space-Content zwischen Members (funktioniert auch wenn Peers nie gleichzeitig online sind). Whitelist-gefiltert.
- **Sync-Modus (c) — P2P Shared-Space** — Direktverbindung zwischen zwei Space-Members via QUIC. Whitelist-gefiltert.
- **Sync-Modus (d) — Federation** — Alice's Sync-Server ↔ Bob's Sync-Server für Content, für den beide autorisiert sind. Whitelist-gefiltert.
- **P2P Owner-Own** — Zwei Geräte desselben Owners direkt verbunden. Voller CRDT-Table-Set via `discover_crdt_tables()` (dynamische Discovery über `haex_hlc`-Column).
- **Shared-Space-Whitelist** — Kanonische Table-Liste die einen Shared-Space-Sync-Kanal (Modi b/c/d) passieren darf. Rust ist Source of Truth (`SPACE_SCOPED_CRDT_TABLES` in `src-tauri/src/crdt/scanner.rs`); TS konsumiert per ts-rs-Binding (Alignment in progress).

## Delete-Handling

- **Owner-Delete-Log** — `haex_deleted_rows`. Owner-only, syncht nicht in Shared Spaces. Bestehend seit Phase-0.
- **Shared-Space-Delete-Log** — `haex_shared_space_deleted_rows`. Per-Space (mit `space_id`-Column), in Shared-Space-Whitelist. Neu (V2-Design 2026-07-29).
- **Register** — `haex_shared_space_sync`. M:N-Mapping von Business-Rows zu Spaces. Per-Space-gefiltert in Whitelist. Push-Deklaration eines Owners ("Ich behaupte: Row R gehört in Space X"), kein Pull-Signal für andere Peers.
- **Unshare vs. Hard-Delete** — Unshare löscht nur einen Register-Eintrag (Business-Row bleibt lokal). Hard-Delete löscht die Business-Row (Register-Einträge cascadieren mit). Aus Peer-Empfänger-Sicht identisches Signal.
- **Compaction-Anchor** — `haex_space_compaction_anchors` pro Space (synced, max-wins-merge, Leader-only advance); Owner-Domain analog mit einem globalen Anchor in `haex_vault_settings`. Verhindert Zombie-Wiederauferstehung nach Retention-Pruning.
- **Register-Check** — Autorisierungs-Gate beim Apply eines Delete-Log-Eintrags: Positive Register-Evidenz für `(target_table, target_row_pks, target_space_id)` in `haex_shared_space_sync` MUSS vorliegen, damit die Business-Row gelöscht wird. Fehlt der Register-Eintrag lokal, ist der Signal-Apply ein No-op — sowohl bei "row shared in another space" (Forgery-Schutz) als auch bei "Race mit local unshare" (Unshare hält die Business-Row per §6.5). Register-Cleanup läuft nur nach positive Gate.

## Datei-Vertraulichkeit (Cloud)

Status: Rust-Primitiven + Decorator gelandet (Phase 4 Runden A–D, PRs #827/#828/#829/#830, Stand 2026-08-25). Offen: Provider-Wiring in `commands.rs::create_provider`, VaultKey-Transport TS→Rust für den own-vault-Pfad, Legacy-Klartext-Migration (Runde E), E2E-Attack-Spec (Runde F).

- **Epoch-Key** — Pro Space und Epoch aus dem MLS-Group-State abgeleiteter symmetrischer Content-Key. Persistiert pro Epoch in `haex_mls_sync_keys` (CRDT-gesynct), abrufbar per `(space_id, epoch)`. Rotiert bei Membership-Änderung; alte Epochs bleiben abrufbar, damit historischer Ciphertext lesbar bleibt.
- **File-Envelope** — Selbstbeschreibendes Rahmenformat eines verschlüsselten Cloud-Objekts: Header (Magic `HXFE`, Version, `epoch`, File-Nonce) gefolgt von Chunks mit je eigenem AEAD-Tag. Der Header nennt die Epoch, damit der Leser den passenden Key wählen kann, ohne Backend-Metadaten zu brauchen. Implementierung: `src-tauri/src/file_sync/crypto/{envelope,chunk,content}.rs`.
- **Content-Objekt** — Cloud-Objekt mit dem verschlüsselten Dateiinhalt, adressiert über einen opaken Object-Key.
- **Metadata-Sidecar** — Kleines Begleitobjekt (`<key>.m`) zum Content-Objekt, gleiches Envelope-Format, enthält Dateiname, Größe, Typ, mtime und Klartext-Hash. Erlaubt Member das Auflisten eines Buckets ohne Download der Inhalte. Implementierung: `src-tauri/src/file_sync/crypto/sidecar.rs`.
- **Opaker Object-Key** — Zufällige ID als Cloud-Objektname (`o/<32-hex>`). Trägt keine Pfad-Information; die Zuordnung Pfad→ID lebt im Metadata-Sidecar und wird lokal in `haex_sync_state_no_sync.object_key` gecached (Migration 0019). Der Storage-Betreiber lernt dadurch weder Dateinamen noch Ordnerstruktur. Implementierung: `src-tauri/src/file_sync/crypto/object_key.rs`.
- **Encrypting Sync Provider** — `SyncProvider`-Decorator, der einen inneren Provider (in Produktion `CloudProvider`) mit Envelope + Sidecar wrappt: `manifest()` meldet Klartextgrößen aus dem lokalen Cache (kein Ciphertext-Größen-Leak in den Diff), `write_file*` sealt Content + Sidecar unter einem frisch geminteten oder wiederverwendeten Object-Key, `read_file*` unsealt streamend, `delete_file` löscht beide Objekte. Bootstrap beim ersten `manifest()`-Aufruf rekonstruiert den lokalen Object-Key-Cache aus den Sidecars des Buckets. Implementierung: `src-tauri/src/file_sync/crypto/provider.rs`. Wird noch nicht in `create_provider` verkabelt (Follow-up).
- **Key-Source (`FileKeySource`)** — Achse, aus der der Decorator den Content-Key wählt: `SpaceEpoch { space_id }` (gewired) resolved den aktuellen Epoch-Key beim Sealen und den historischen Epoch-Key beim Öffnen. `VaultKey` (Placeholder) ist der own-vault-Pfad — Rust hat heute keinen `vault_key`-Handle (lebt in TS `vaultKeyCache`), Transport TS→Rust ist eigener Follow-up-PR.

## Space-Rollen

- **Space-Infrastruktur-Tabellen** — Die 5 Tabellen in `SPACE_SCOPED_CRDT_TABLES` (`haex_space_devices`, `haex_space_members`, `haex_peer_shares`, `haex_mls_sync_keys`, `haex_device_mls_enrollments`). Bootstrap-scope, NICHT gleichzusetzen mit "Core" (= alle `haex_*` Tables) oder mit dem Register.
- **Leader (Space-Kontext)** — In Modus (c) der elected P2P-Coordinator; in (b) der Sync-Server; in (d) die eigene Server-Instanz. Rolle: Compaction-Anchor advance, Push-Reject-Enforcement bei HLC unter Anchor, Peer-Cursor-Tracking.
- **Owner** — Vault-Besitzer. Kann mehrere eigene Geräte haben, die sich untereinander vollständig vertrauen (Modus a + P2P Owner-Own).
- **Member (Space-Kontext)** — DID mit UCAN-Delegation innerhalb eines Spaces. **Write == Delete** (kein separates Delete-Cap — Write-Member kann durch Overwrite ohnehin inhaltlich löschen).
- **Capability / CapabilitySet** — `Cap` (`Read`, `Write`, `Invite`, `Admin`) ist **orthogonal**, keine Rangfolge und keine Implikation: ein Member kann `Write` ohne `Read` halten. Ein `CapabilitySet` ist die Menge der gehaltenen Caps, jeder Eintrag mit eigenem `delegatable`-Flag (ob er weiterdelegiert werden darf). Definition: `src-tauri/src/ucan/capability_set.rs`. Das frühere `CapabilityLevel`-Lattice ist entfernt.
