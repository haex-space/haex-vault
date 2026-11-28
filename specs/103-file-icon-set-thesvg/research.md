# Research: A coloured file icon set from theSVG

Spec: [spec.md](./spec.md). Measured on 2026-11-28 against `glincker/thesvg` at commit
`a417e5a090a1258d727f5837f1413788b6337845`.

## R1 — What to take from thesvg

**Decision**: the **UI icons** (`src/data/ui/looks/fill.json`: 1,189 icons as slug → SVG text, one file per
look), not the brand logos.

**Rationale**: the operator wants one more complete set like Lucide, Phosphor and Material. The brand
logos cover technologies, not kinds of file (no folder, no PDF, no archive), and carry per-icon
submitter-declared licenses and trademark terms. The UI icons are original drawings by thesvg
(`LICENSING.md` §1: "original drawings made for theSVG, not brand marks, so there is no trademark to
consider … released under the MIT license"); `LICENSE` is MIT, "Copyright (c) 2025 thesvg.org". Every
icon we need is a glyph of a `src/data/ui/sets/*.json` file (checked).

**Alternatives**: the brand logos (an earlier draft of this spec; rejected: no roles, license and
trademark work, not what was asked); the live site or npm package `@thesvg/ui` (not pinnable / adds
a dependency for a build-time need).

## R2 — Which icons exist for the kinds of file

All of these exist in the fill look and are glyphs of the source's own set files: `folder`, `file`,
`image`, `video`, `audio-lines`, `pdf-file`, `file-text`, `file-code`, `archive-zip`, `json-file`,
`markdown-file`, `config-file`, `env-file`, `log-file`, `lockfile`, `license-file`, `readme-file`,
`presentation-file`, `spreadsheet`, `film`, `file-archive`, `file-image`, `database`, `terminal`.
Their drawings are generic (a PDF is a page with a stroke, not a "PDF" label), so the set cannot
distinguish `.rs` from `.py`; both get `file-code`. That is the nature of a UI icon set.

## R3 — Colour

**Decision**: replace `currentColor` in each icon with a fixed colour from a checked-in table, by
kind: folder `#42a5f5`, generic file `#90a4ae`, image `#26a69a`, video `#ff9800`, audio `#ec407a`,
PDF `#e53935`, text `#64b5f6`, code `#7e57c2`, archive `#afb42b`, JSON `#f9a825`, Markdown `#0288d1`,
config `#78909c`, env `#66bb6a`, log `#8d6e63`, lock `#ffb300`, licence `#ffb300`, readme `#29b6f6`,
presentation `#ff7043`, spreadsheet `#43a047`. The palette is Material's, which already reads on both
schemes.

**Check**: the relative luminance of every colour must lie in 0.10 … 0.65 (the measured table above:
0.15 … 0.54), so no separate light images are needed (FR-005). The fill look keeps its inner details
as mask cut-outs, so they show the background and stay visible on both schemes (rendered and looked
at: fill row of the contact sheet).

**Alternatives**: a second set of images for the light scheme (double the files for no gain); `mono`
(the other sets are coloured and the operator asked for colour).

## R4 — Extensions and names

**Decision**: superseded by R7. First version: a checked-in `thesvg.mapping.json` (extension or name →
slug) seeded from `phosphor.mapping.json`; replaced the same day by the shared mapping of R7.

## R5 — Safety of the bundled SVGs

**Decision**: no second sanitiser. `build_bundled_packs` packs the source directory with `pack_dir`,
and `every_bundled_pack_passes_the_checks` / `…is_what_its_source_directory_packs_to` read it back through
`open_pack`, which applies `svg.rs`. The checks allow `defs`, `mask`, `linearGradient` and `stop`, the
elements the fill look uses (checked in `svg.rs`'s allow-list).

## R6 — Where the set appears

`BuiltinSet::Thesvg` (key `thesvg`, selection value `builtin:thesvg`), after Material in
`BuiltinSet::ALL`; display name "theSVG"; i18n `settings.iconSet.sets.thesvg`. `ts-rs` regenerates
`BuiltinSet.ts`; `fileIcons.ts` gets `'thesvg'`. The default stays Material. An older build that reads
`builtin:thesvg` shows the fallback (`parse_selection` returns `None`; spec 053 FR-020).

## R7 — License page and spec 053

`Pack::license_text` already shows `LICENSE`, `LICENSE-*` and `NOTICE` of a pack: the pack carries the
MIT text copied from the clone's `LICENSE` and a `NOTICE` with source and commit. Spec 053 gets only an
entry in FR-007's list; FR-017 is unchanged because MIT is already accepted.

## R8 — Build script layout

`scripts/icon-sets/prepare.ts` is 417 lines and the limit is 500. Move the shared helpers to
`scripts/icon-sets/common.ts`, add `scripts/icon-sets/thesvg.ts`, and let `prepare.ts` dispatch
`thesvg`. Pure parts (colour check, mapping rules) are tested with `node --test`
(`scripts/check-thesvg-icons.ts`, wired like `check:file-icons`).

## R7 — One mapping of kinds for every set (operator, 2026-11-28)

**Decision**: a set-independent `src-tauri/icon-sets/kinds.json` maps extensions, file names and folder
names (English, German, French, Spanish) to **kinds** (`rust`, `pdf`, `readme`, `folder-downloads`); a kind
may have a parent (`rust` → `code`, `csv` → `spreadsheet`). A set has only a table kind → image
(`thesvg.kinds.json`, `phosphor.kinds.json`); a kind without an image takes the one of its nearest parent,
and without one the entry shows the generic image. Material keeps its upstream mapping and only receives
the folder names of the shared kinds. The code is `scripts/icon-sets/kinds.ts`, used by `prepare.ts` and
`thesvg.ts`, checked by `scripts/check-icon-kinds.ts`.

**Rationale**: the operator wants to add further sets later and reuse the same mapping. Phosphor draws one
image per language (`file-rs`, `file-ts`), theSVG only one for all code: parents let each set be as fine
as it is. The localized folder names existed twice (a list in `prepare.ts` for Material and the first
version for theSVG); now they exist once.

**Alternatives considered**: one flat `extension → slug` file per set (the first version; every new set
repeats 200 entries and drifts); one flat kind per extension without parents (Phosphor's per-language
images would be lost or every set would need an image for every kind).

**Consequence**: Phosphor gains extensions and file names from the shared mapping (160 extensions and 20
names instead of 82 extensions); its images stay the same. Material's pack is unchanged.
