# Contract: Format verschlüsselter Ordner `HXEF` v1

Dieses Dokument ist die Quelle für FR-042 von Spec 048 und gilt für jeden, der `HXEF` liest oder
schreibt ([ADR 0013](../adr/0013-encrypted-folder-format.md)). Die Testvektoren liegen in
`src-tauri/tests/fixtures/encrypted/v1-vectors.json`, die Umsetzung in
`src-tauri/src/files/encrypted/format/`. Begründungen stehen in
[research.md](../../specs/048-encrypted-folders/research.md) R1 bis R8.

## Bausteine

- **AEAD**: XChaCha20-Poly1305 (24-Byte-Nonce, 16-Byte-Tag).
- **KDF**: HKDF-SHA256. `kdf(ikm, info)` heißt immer: Salt = die 5 Byte `holzi`, Länge 32.
- **Kennungen**: 16 zufällige Bytes; als Objektname in Base32 nach RFC 4648 ohne Auffüllung,
  Kleinbuchstaben (26 Zeichen).
- **AAD**: Kennungen stehen darin als rohe 16 Byte, Wörter wie `name` als UTF-8 ohne Längenangabe.
- **Zahlen**: Little Endian.
- **Hex** und **Base64** (in Begleitdateien): Hex in Kleinbuchstaben; Base64 nach RFC 4648 mit
  Standardalphabet und Auffüllung.
- **Konstanten**: `MAGIC` = `HXEF` (4 Byte), `VERSION` = `0x01`, `CHUNK` = 65 536 Byte Klartext.
- Typ-Bytes: `0x00` Kopf, `0x01` Inhalt, `0x02` Begleitdatei.

## Schlüssel

| Name     | Herkunft                                                          | Zweck                                   |
| -------- | ----------------------------------------------------------------- | --------------------------------------- |
| `VCK_g`  | Inhaltsschlüssel der Vault, Generation `g` (`sync::content_keys`) | nur Eingabe für `KEK_g`                 |
| `KEK_g`  | `kdf(VCK_g, "holzi/encrypted-folder/kek/v1")`                     | verpackt `FK` im Kopf                   |
| `FK`     | 32 zufällige Bytes je Ordner                                      | nur Eingabe für die zwei Unterschlüssel |
| `K_meta` | `kdf(FK, "holzi/encrypted-folder/meta/v1")`                       | Name im Kopf, Begleitdateien            |
| `K_wrap` | `kdf(FK, "holzi/encrypted-folder/wrap/v1")`                       | verpackt Dateischlüssel                 |
| `DEK`    | 32 zufällige Bytes je Inhaltsobjekt (jede Fassung neu)            | Inhalt                                  |

`KEK_g` und `VCK_g` verlassen die Vault nie. Teilen (FR-040) fügt dem Kopf einen weiteren Empfänger
hinzu, der `FK` unter einem anderen Schlüssel trägt.

## Objekte im Speicher

Ein verschlüsselter Ordner mit der Kennung `fid` liegt im Elternpräfix `E` (leer an der Wurzel des
Buckets) unter `P = E + base32(fid) + ".hxef/"`:

| Objekt        | Name                     | Inhalt                            |
| ------------- | ------------------------ | --------------------------------- |
| Kopf          | `P + "h"`                | siehe unten                       |
| Begleitdatei  | `P + "m/" + base32(sid)` | je Fassung eines Eintrags         |
| Inhaltsobjekt | `P + "c/" + base32(oid)` | je Fassung oder Kopie einer Datei |

Sonst liegt nichts unter `P`. Content-Type aller Objekte ist `application/octet-stream`; holzi setzt
keine eigenen Metadaten.

Als verschlüsselter Ordner gilt nur ein Präfix aus genau 26 Zeichen Base32 (Kleinbuchstaben) und
`.hxef/`. Einen gewöhnlichen Ordner oder eine Datei mit einem Namen dieser Form legt holzi nicht an
(Fehler `invalidName`), damit er nicht als beschädigter verschlüsselter Ordner erscheint.

`oid` ist der Objektname eines Inhaltsobjekts, `cid` (im Kopf des Inhaltsobjekts) die Kennung seines
Inhalts. Beim Schreiben einer neuen Fassung sind beide gleich und neu. Eine Kopie beim Anbieter
(innerhalb eines verschlüsselten Ordners oder in einen anderen derselben Vault) bekommt eine neue
`oid` und behält `cid` und `DEK`, weil `cid` in jedem Block steckt; die neue Begleitdatei verpackt
`DEK` für ihren Eintrag und ihren Ordner neu. So gehört jedes Inhaltsobjekt zu genau einer
Begleitdatei, und Löschen trifft nie den Inhalt eines anderen Eintrags.

## Kopf

```text
MAGIC(4) | VERSION(1) | 0x00 | fid(16) | n(1) | Empfänger × n | name_nonce(24) | name_ct
Empfänger = art(1) | len(2) | daten(len)
art 0x00:  daten = key_id(16) | wrap_nonce(24) | wrapped_fk(48)        len = 88
```

- `art` `0x00` = eigene Vault; `key_id` ist die Kennung von `VCK_g` (`content_keys`, 16 Byte). Andere
  Arten sind für Spaces reserviert und dürfen andere Längen haben (etwa für einen öffentlichen
  Schlüssel des Mitglieds); ein Leser überspringt Empfänger, die er nicht kennt, anhand von `len`. Ein
  Empfänger der Art `0x00` mit `len` ≠ 88 macht den Kopf „beschädigt“.
- `wrapped_fk = AEAD(KEK_g, wrap_nonce, FK, aad = MAGIC|VERSION|0x00|fid|art|key_id)`.
- `name_ct = AEAD(K_meta, name_nonce, utf8(Name), aad = MAGIC|VERSION|0x00|fid|"name")`.
- Umbenennen schreibt den Kopf mit neuem `name_nonce` neu; die Empfänger bleiben.
- Ein Leser lehnt ab: andere `MAGIC`, `VERSION` > 1 (Zustand „neuere Formatversion“), `fid` passt nicht
  zum Präfix, kein entpackbarer Empfänger (Zustand „andere Vault“), Name nicht zu öffnen
  („beschädigt“).
  Ein Empfänger der Art `0x00`, dessen `key_id` der Leser kennt, der sich aber nicht entpacken
  lässt, zählt dabei wie ein fremder: Gibt es keinen anderen, ist der Zustand „andere Vault“.

## Inhaltsobjekt

```text
Kopf:   MAGIC(4) | VERSION(1) | 0x01 | cid(16) | nonce_prefix(16)        = 38 Byte
Block i: AEAD(DEK, nonce_prefix | u64(i), block_i, aad = Kopf(38) | last_i)
```

- `last_i` = `0x01` für den letzten Block, sonst `0x00`.
- Blöcke haben `CHUNK` Byte Klartext, der letzte 1 bis `CHUNK`. Jedes Objekt hat mindestens einen
  Block; nur eine leere Datei hat einen leeren (letzten) Block, also Kopf und 16 Byte. Eine Datei
  mit einem Vielfachen von `CHUNK` Byte endet mit einem vollen Block mit `last = 0x01`, ohne leeren
  Block dahinter; ein Schreiber hält dafür einen Block zurück, bis er weiß, ob noch einer folgt.
- Jeder Block wird unter seinem `DEK` genau einmal verschlüsselt. Ein neuer Versuch (Netzfehler,
  abgebrochener Teil eines mehrteiligen Uploads) schickt dieselben verschlüsselten Bytes noch einmal
  oder beginnt ein neues Inhaltsobjekt mit neuem `DEK`, neuer `cid` und neuem `nonce_prefix`. Den
  Klartext neu zu lesen und unter demselben `DEK` neu zu verschlüsseln ist verboten, weil sich die
  Datei inzwischen geändert haben kann (Nonce-Wiederverwendung).
- Länge: `38 + 16 × k + Größe` mit `k = max(1, ⌈Größe / CHUNK⌉)`.
- Teilbereich `[a, b)` des Klartexts: Blöcke `⌊a / CHUNK⌋` bis `⌊(b − 1) / CHUNK⌋`, Block `i` beginnt
  bei `38 + i × (CHUNK + 16)`. Der Kopf wird einmal je geöffneter Datei gelesen.
- Ein Leser prüft: `cid` im Kopf = `cid` aus der Begleitdatei; Länge des Objekts passt zur Größe aus
  der Begleitdatei; der letzte Block trägt `last = 0x01`, kein anderer. Beim vollständigen Lesen
  (Herunterladen, Transfer) zusätzlich SHA-256 des Klartexts.

## Begleitdatei

```text
MAGIC(4) | VERSION(1) | 0x02 | sid(16) | nonce(24) | AEAD(K_meta, nonce, json, aad = MAGIC|VERSION|0x02|fid|sid)
```

Ein Leser prüft: `sid` im Objekt = `sid` aus dem Objektnamen (`P + "m/" + base32(sid)`). Weil
`sid` und `fid` im AAD stehen, lässt sich eine Begleitdatei weder unter einen anderen Namen noch in
einen anderen Ordner kopieren.

`json` ist UTF-8 ohne Leerraum, Felder in dieser Reihenfolge (ein unbekanntes Feld macht die
Begleitdatei ungültig, ein neues Feld braucht eine neue Formatversion):

| Feld       | Typ                | Regel                                                                          |
| ---------- | ------------------ | ------------------------------------------------------------------------------ |
| `v`        | Zahl               | `1`                                                                            |
| `entry`    | Base32             | Kennung des Eintrags, bleibt über Fassungen gleich                             |
| `parent`   | Base32             | Kennung des Elterneintrags; `fid` an der Wurzel des Ordners                    |
| `name`     | Text               | Name nach den Regeln von 044 (`check_name`)                                    |
| `kind`     | `file` \| `folder` |                                                                                |
| `revision` | Zahl               | 1 bei der ersten Fassung, dann +1                                              |
| `base`     | Base32 \| `null`   | `sid` der Begleitdatei, die diese ersetzt                                      |
| `written`  | Zahl               | Zeit des Schreibens, ms seit 1970                                              |
| `size`     | Zahl               | nur `file`: Größe des Klartexts                                                |
| `modified` | Zahl               | nur `file`: Änderungszeit der Datei, ms                                        |
| `type`     | Text \| `null`     | nur `file`: Inhaltstyp, wenn bekannt                                           |
| `sha256`   | Hex                | nur `file`: Prüfsumme des Klartexts                                            |
| `content`  | Base32             | nur `file`: `oid`, Objektname des Inhaltsobjekts                               |
| `cid`      | Base32             | nur `file`: `cid` aus dem Kopf des Inhaltsobjekts                              |
| `dek`      | Base64             | nur `file`: `nonce(24) \| AEAD(K_wrap, nonce, DEK, aad = fid \| entry \| cid)` |

## Regeln für Leser und Schreiber

- **Ansicht**: alle Begleitdateien laden. Gibt es für eine `entry` mehrere, gilt die Fassung, die keine
  andere als `base` hat. Haben zwei Fassungen dieselbe `base`, ist das ein Konflikt: beide gelten, die
  mit dem kleineren `written` wird als „Name (Konflikt JJJJ-MM-TT hh-mm)“ gezeigt. Gleiche Namen
  verschiedener Einträge im selben Elterneintrag werden genauso benannt.
- **Einträge ohne Eltern** (Elterneintrag gelöscht, während ein anderes Gerät etwas hineinlegte)
  erscheinen im virtuellen Ordner „Wiederhergestellt“ an der Wurzel des verschlüsselten Ordners.
- **Schreiben**: zuerst das neue Inhaltsobjekt ganz, dann die neue Begleitdatei, danach alte
  Begleitdatei und altes Inhaltsobjekt löschen.
- **Kopieren beim Anbieter** (044-Transfer, FR-018, FR-020): das Inhaltsobjekt unter einer neuen
  `oid` im Zielordner kopieren, dann eine neue Begleitdatei mit neuer `entry`, `content` = neue `oid`,
  `cid` wie im Original und `dek` unter `K_wrap` des Zielordners neu verpackt.
- **Aufräumen**: Begleitdateien, die eine andere als `base` hat, und Inhaltsobjekte ohne Verweis
  löscht holzi, wenn ihre Änderungszeit beim Anbieter älter als 24 Stunden ist.
- **Löschen eines Eintrags**: Inhaltsobjekte und Begleitdateien aller Nachkommen, zuletzt die eigene.
- **Löschen des Ordners**: alle Objekte unter `P`, der Kopf zuletzt.

## Testvektoren

`v1-vectors.json` enthält mit festen Schlüsseln, Kennungen und Nonces (wie sie entstanden sind, steht
in ihrem Feld `_comment`; die Plattform-Probe der CI prüft sie auch unter Windows, macOS und
Android):

1. `KEK_g`, `K_meta`, `K_wrap` aus festen `VCK_g` und `FK`.
2. Einen Kopf mit einem Empfänger und Namen „Unterlagen“.
3. Inhaltsobjekte mit 0, 1, 65 536 und 65 537 Byte Klartext.
4. Eine Begleitdatei für eine Datei, eine für ihre Kopie (neue `oid`, gleiche `cid`) und eine für
   einen Ordner.
5. Negativfälle mit erwarteter Ablehnung: verändertes Byte, gekürzt an einer Blockgrenze, verlängert
   (auch um einen leeren Block nach einem vollen letzten), zwei Blöcke vertauscht, `last` am falschen
   Block, Inhaltsobjekt einer anderen Datei, Begleitdatei eines anderen Ordners, Begleitdatei unter
   anderem `sid`.
