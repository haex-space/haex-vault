# Research: Modellsuche im Chat

## R1: Fehlertoleranter Treffer-Abgleich

**Decision**: `fuse.js` (neue, kleine, abhängigkeitsfreie npm-Bibliothek)
gegen Anbieter- und Modellname, mit `includeScore` für die Reihenfolge
innerhalb einer Anbieter-Gruppe.

**Rationale**: Spec 031 verlangt in FR-003 nicht-zusammenhängende
Teilfolgen-Treffer ("gpt4o" findet "GPT-4o") **und** in SC-002/Acceptance
Scenario 2 Toleranz gegenüber einem einzelnen Tippfehler. Eine reine
Teilfolgen-Prüfung (Zeichen des Suchbegriffs kommen der Reihe nach im
Zielstring vor) deckt Auslassungs- und Umstellungs-Tippfehler ab, aber
**keine** Vertauschung/Ersetzung eines Buchstabens (Beispiel: Suchbegriff
„cluade" gegen „claude" — „u" und „a" sind vertauscht, keine gültige
Teilfolge in beide Richtungen). Ein korrekter, review-fähiger
Distanz-basierter Score ist genau die Fähigkeit, die eine fokussierte
Bibliothek zuverlässiger liefert als eine neu geschriebene Distanzfunktion,
die eigene Tests und Abstimmung bräuchte (spaex-Constitution SHOULD: „keep
any dependency addition justified by a real capability ... rather than a
shorter one-off expression").

**Alternatives considered**:

- **Selbstgeschriebene Teilfolgen-Suche** (kein neuer Dependency): verworfen,
  weil sie SC-002 (Tippfehler-Toleranz) nicht durchgängig erfüllt (siehe
  oben).
- **`src/lib/settings/search.ts` (`searchSettings`) verallgemeinern**: Die
  bestehende Einstellungssuche (Spec 023) nutzt Wort-Teilstring-Treffer mit
  eigenem Ranking, keine zeichenweise Fuzzy-Suche — löst dieselbe Klasse von
  Tippfehlern nicht (ein Suchwort muss weiterhin zusammenhängend im Text
  vorkommen). Außerdem ist sie an `SettingsLocation` gekoppelt; eine
  Verallgemeinerung für `ModelGroup[]` wäre ein eigener, nicht triviale
  Umbau an bereits ausgeliefertem Code für einen Gewinn, der die
  FR-003-Anforderung trotzdem nicht vollständig deckt.
- **Eigener Levenshtein-Scorer**: verworfen als „clever" statt „boring"
  (ponytail-Leitlinie); eine verbreitete, gepflegte Bibliothek ist die
  langweiligere und robustere Wahl für eine Fähigkeit, die leicht subtil
  falsch zu implementieren ist.

## R2: Einbindung des Suchfelds in die bestehende Auswahl

**Decision**: Ein natives Textfeld als erstes Kind in
`ShadcnSelectContent`, mit `@keydown` so behandelt, dass Zeichen- und
Rücktaste-Eingaben nicht an Rekas eigene Sprung-zu-Buchstabe-Suche der
`Select`-Liste weitergereicht werden (`stopPropagation`), während
Pfeiltasten, Eingabe und Escape normal an die bestehende Listbox
durchgereicht werden. Modelle/Anbieter-Gruppen werden weiterhin mit
`v-for`/`v-if` gerendert, gefiltert über eine `computed`, die
`modelSearch.ts` (R1) aufruft; eine ausgeblendete Gruppe verschwindet
vollständig aus dem DOM, wodurch Rekas Tastatur-Navigation automatisch nur
sichtbare Einträge sieht.

**Rationale**: Alle bestehenden Auswahllisten in haex-vault (`ComposerControl.vue`,
`settings/Select.vue`, hier) nutzen ausschließlich die `Shadcn*`-Hüllen aus
dem externen haex-ui-Nuxt-Layer (`github:haex-space/haextension/packages/
haex-ui`, gepinnt in `nuxt.config.ts`); nirgends im Projekt wird `reka-ui`
direkt importiert. Diese Spec ändert nur haex-vault, nicht den externen Layer.

**Alternatives considered**:

- **`reka-ui`s `Combobox*`-Primitive direkt verwenden** (technisch für genau
  diesen Fall gebaut, im installierten `reka-ui` bereits vorhanden): verworfen,
  weil damit erstmals ein haex-vault-Component direkt an rohen `reka-ui`-Bausteinen
  vorbei an der gemeinsamen `Shadcn*`-Hülle bedient würde — ein Bruch mit der
  einzigen im Projekt etablierten Konvention für Auswahllisten. Der saubere
  Weg (eine `ShadcnCombobox*`-Familie im haex-ui-Layer) ist eine
  Cross-Repo-Änderung an einem separaten, von haex-space verwalteten Repo und
  damit außerhalb des Umfangs dieser Spec.
- **Neue lokale Wrapper-Komponente um `reka-ui`s Combobox** (nur in haex-vault,
  ohne den haex-ui-Layer zu ändern): verworfen aus demselben Grund — sie
  würde dauerhaft neben der `Shadcn*`-Familie als zweiter, abweichender
  Auswahl-Baustein bestehen bleiben.

## R3: Kein-Treffer-Hinweis

**Decision**: Ein einzelnes `<div>` mit kurzem, übersetztem Text
(`chat.composer.settingsPopover.modelSearch.noResults` o. ä.), gerendert
anstelle der Anbieter-Gruppen, wenn die gefilterte Liste leer ist — keine
neue Komponente, keine Icon-Illustration (Konsistenz mit dem knappen,
unaufdringlichen Ton anderer Hinweise im Composer, etwa dem
Modell-Platzhalter `chat.model.choose`).

**Rationale**: Spec verlangt nur „einen kurzen, unaufdringlichen Hinweis"
(FR-005); ein größeres Empty-State-Muster (Icon + Beschreibung, wie
`DownloadModels.vue`s leere Trefferliste) wäre für ein Dropdown mit
begrenzter Höhe unverhältnismäßig.

**Alternatives considered**:

- Das bestehende, größere Empty-State-Muster aus den Einstellungen
  (Icon + Text + Aktion) übernehmen: verworfen als zu schwer für einen
  Popover-Kontext und ohne sinnvolle Aktion (es gibt nichts zu „herunterladen"
  o. Ä. an dieser Stelle).
