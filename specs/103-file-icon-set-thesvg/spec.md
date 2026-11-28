# Feature Specification: A coloured file icon set from theSVG ("theSVG")

**Feature Branch**: `103-file-icon-set-thesvg`

**Created**: 2026-11-28

**Status**: Draft

**Input**: User description: "ich würde gerne weitere file icons zur auswahl hinzufügen. können wir
https://thesvg.org/ nutzen?" (operator, 2026-11-28); "ich brauche das nur für file icons"; "ich möchte
nur ein weiteres icon set hinzufügen, so wie wir bereits 3 haben"; style: "ich möchte bunte icons haben".

## Clarifications

### Session 2026-11-28

- Q: What from thesvg.org is used? → A: Its own **UI icons** (original drawings, MIT, no trademarks),
  not its brand logos. The brand logos (about 4,800, mostly under submitter-declared licenses and
  trademark-only terms) are not used; an earlier draft of this spec planned a logo set and was dropped
  because the operator wants one more set like the three that exist (a complete set with folder, file,
  image, video, audio, PDF, text and archive images), not a logo overlay.
- Q: One set or one per style? → A: **One** new set. thesvg draws every icon in seven looks (outline,
  duotone, fill, glass, glass outline, depth, offset); the operator chose **fill** (solid, strong
  shapes, closest to Material).
- Q: Coloured? → A: Yes, "bunte Icons" (operator, 2026-11-28). The source icons are one-colour
  (`currentColor`); the build gives each icon a fixed colour by its kind (folder blue, image teal,
  video orange, audio pink, PDF red, text light blue, code purple, archive olive …), like Material.
  Colouring is allowed because the drawings are MIT.
- Q: Does spec 053's license rule change? → A: No. The icons are MIT, so FR-017 of spec 053 holds as
  written. Only FR-007 (the list of bundled sets) gains the new set.

## Terms

- **theSVG** (the set): the new bundled icon set; picker label "theSVG".
- **Source**: the `glincker/thesvg` repository, pinned to one commit; the file
  `src/data/ui/looks/fill.json` (slug → SVG text) holds the fill look of every UI icon.
- **Role**, **State image**, **Fallback set**: as in spec 053.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Choose a fourth, coloured icon set (Priority: P1)

A user opens the settings, finds "theSVG" next to Lucide, Phosphor and Material, with a preview row of
coloured images (folder, image, video, audio, PDF, text, code, archive), chooses it, and the file
browser shows folders, files, images, videos, audio files, PDFs, documents, code files and archives with
those images everywhere an entry image is shown.

**Why this priority**: This is the request: one more set, selectable the same way as the others.

**Independent Test**: Choose "theSVG", open a folder with one file per kind; each kind shows its own
coloured image, an encrypted folder shows the green state image, and the choice is still there after a
restart and on a second device.

**Acceptance Scenarios**:

1. **Given** the settings, **When** the user opens "Icon-Set für Dateien", **Then** the list shows
   Lucide, Phosphor, Material and theSVG, each with a preview and its author and license, and the
   default is still Material.
2. **Given** theSVG is chosen, **When** a folder holds `a.png`, `a.mp4`, `a.mp3`, `a.pdf`, `a.txt`,
   `a.rs`, `a.zip` and a subfolder, **Then** each shows the image of its kind (spec 053 FR-003).
3. **Given** theSVG is chosen, **When** an extension or name has no mapping, **Then** the entry shows the
   set's generic file image; **When** the entry is encrypted, locked, without access or damaged,
   **Then** it shows HaexVault's own state image (spec 053 FR-005).
4. **Given** the light and the dark colour scheme, **When** any image of the set is shown, **Then** it is
   readable on both (FR-005).

---

### User Story 2 - See the source and license (Priority: P2)

A user opens "Lizenzen" and finds theSVG with its author ("thesvg.org"), the MIT license text, the
source address and the pinned version.

**Why this priority**: Required by spec 053 FR-017 for every bundled set; the MIT text must be shipped.

**Independent Test**: Open "Lizenzen"; the set appears with author, "MIT", source and the full text.

**Acceptance Scenarios**:

1. **Given** the app contains theSVG, **When** the user opens "Lizenzen", **Then** the page lists it
   with author, license, source address and the full MIT text of the source repository.
2. **Given** the settings list, **When** theSVG is shown, **Then** its author and license are shown
   there too (as for the other sets).

---

### User Story 3 - Rebuild the set from the pinned source (Priority: P3)

A maintainer rebuilds the set from the pinned thesvg clone and gets the same result; the build refuses a
missing icon, an unreadable colour, a clone at the wrong commit and a file that is not safe to show.

**Why this priority**: Keeps the bundle reproducible and honest, like Phosphor and Material.

**Independent Test**: Run the build twice: no change after the second run; point it at a clone with a
mapping to a missing icon: it fails and names the icon.

**Acceptance Scenarios**:

1. **Given** the pinned clone, **When** the build runs twice, **Then** the checked-in result is
   identical.
2. **Given** a mapping that names an icon the source does not have, **When** the build runs, **Then** it
   stops and names the icon.
3. **Given** a colour that is hard to read on the dark or the light scheme, **When** the build runs,
   **Then** it stops and names the icon and colour.

---

### Edge Cases

- A device runs an older build without theSVG while the synced choice is theSVG: the existing rule for
  an unavailable set applies (spec 053 FR-020); the fallback set shows.
- An extension is written in capitals (`A.PDF`): names and extensions are matched in lower case, as for
  the other sets.
- A new thesvg release redraws an icon: nothing changes until someone re-pins the source; the pinned
  commit and the checked-in result are the truth.
- The source `fill` look uses masks and gradients inside the SVG: they are allowed by the image checks
  (spec 053 FR-011) and the images are shown as images only, never inlined (FR-010).

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: HaexVault MUST ship a fourth bundled set "theSVG" next to Lucide, Phosphor and Material,
  selectable like the others (`files.iconSet`, vault preference, synced; spec 053 FR-001, FR-015) with
  a preview (spec 053 FR-021). The default set stays Material.
- **FR-002**: The set MUST give an image to these roles: folder, generic file, image, video, audio, PDF,
  and document/text; and MUST map file extensions and file names to the image of their kind: images,
  video, audio, PDF, text and office documents, spreadsheets, presentations, source code, JSON,
  Markdown, configuration, environment, log, lock, licence and readme files, and archives. The set MUST
  also give the places of the side bar (home, downloads, pictures, documents, desktop, videos, music) an
  image of their own, matched by folder name like Material does (operator, 2026-11-28: one folder image
  for all of them looked wrong). The mapping MUST be a checked-in list from which the build runs.
- **FR-003**: The five state images of HaexVault (encrypted, locked, no access, locked file, damaged)
  MUST be part of the set exactly as in Material and Phosphor (spec 053 FR-005).
- **FR-004**: The set MUST use the **fill** look of the source, in colour: each image gets the colour of
  its kind, fixed at build time; no image is drawn by hand and no image of another look is used.
- **FR-005**: Every colour MUST be readable on the dark and on the light colour scheme: its relative
  luminance MUST lie between 0.10 and 0.65 (the build checks it); the set therefore has no separate
  light images.
- **FR-006**: The set MUST be built by `scripts/icon-sets/prepare.ts` from the source pinned to one
  commit, with a checked-in mapping, role table and colour table, and a checked-in result and pack,
  so each change shows in a diff (spec 053 research R6). The script MUST take the clone path from the
  command line only, record no local path, and the build MUST fail if the clone is at another commit.
- **FR-007**: The build MUST fail, naming it, when a mapped icon does not exist in the source, when a
  colour fails FR-005, when an extension or name maps twice, or when an image does not pass the checks
  of spec 053 FR-010 to FR-013 (the same Rust checks, no second implementation).
- **FR-008**: The pack MUST carry the source's MIT license text and a note of author, source address and
  pinned commit; the "Lizenzen" page and the settings list MUST show them (spec 053 FR-017).
- **FR-009**: Spec 053 MUST be updated where it lists the bundled sets (FR-007) and its text on the
  license page and the picker; FR-017 stays as it is.
- **FR-010**: The set MUST work on every device and platform HaexVault runs on; no logic may depend on
  the platform (iOS stays deferred).
- **FR-011**: Extensions and agents MUST NOT read, change or choose the set (spec 053 FR-016).

### Key Entities

- **theSVG set**: a bundled, non-deletable, non-synced icon set (the choice of it is synced) of roughly
  twenty coloured images, a mapping of about a hundred extensions and names, and the five state images.
- **Mapping file**: extension, file name or folder name → source icon slug; **role table**: role → slug; **colour table**:
  slug → colour. Checked in, input of the build.

## Relation to existing specs

- **Spec 053** (file icon sets): this spec adds a set in its format and rules; only the list of bundled
  sets (FR-007) changes. License rule, format, lazy loading, sync, fallback and import apply unchanged.
- **Spec 048**: state images unchanged. **Spec 023**: the picker lists one more row.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: With theSVG chosen, at least 90 file extensions and names show an image of their kind
  instead of the generic file image, and every role image and state image resolves in both schemes.
- **SC-002**: Every image of the set passes the same safety checks as a user's imported set (100 %,
  checked by an automated test on the bundled pack).
- **SC-003**: Rebuilding from the pinned source twice gives byte-identical files.
- **SC-004**: Opening a folder of 1,000 entries is no slower than with Material (the same lazy loading
  applies; the pack is a fraction of Material's size).
- **SC-005**: Every image is readable on both colour schemes (the build enforces the luminance range;
  the operator looks at it once on both schemes).

## Assumptions

- The UI icons of thesvg are original drawings under the MIT license (thesvg `LICENSING.md` §1 and the
  package README), copyright "thesvg.org"; colouring them is a permitted modification.
- The icons for the kinds of file needed exist in the source (checked at planning, research R2); a
  kind without a good icon uses the generic file image.
- "Marken" brand logos for programming languages are out of scope; they stay in Material and Phosphor.
- iOS is out of scope as for the other icon work (decision 2026-11-23).
