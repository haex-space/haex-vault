# Data Model: theSVG icon set

No database table, migration or sync change: the set is built into the app like Phosphor and Material.

## Interface change

| Where                                                             | Before                                                                    | After                              |
| ----------------------------------------------------------------- | ------------------------------------------------------------------------- | ---------------------------------- |
| `BuiltinSet` (Rust, `ts-rs` → `src/types/bindings/BuiltinSet.ts`) | `lucide`, `phosphor`, `material`                                          | adds `thesvg`                      |
| Vault preference `files.iconSet`                                  | `builtin:lucide`, `builtin:phosphor`, `builtin:material`, `custom:<uuid>` | adds `builtin:thesvg`              |
| `BuiltinSet::ALL`                                                 | Lucide, Phosphor, Material                                                | Lucide, Phosphor, Material, theSVG |
| Default set                                                       | Material                                                                  | unchanged                          |

## Checked-in build inputs (`src-tauri/icon-sets/`)

- `kinds.json` (shared by all sets, `scripts/icon-sets/kinds.ts`): `{ "kinds": { "<kind>": { "parent"?: "<kind>", "extensions"?: ["<no dot>"], "names"?: ["<lower case>"] } } }`. A kind whose id starts with
  `folder-` carries folder names, has no parent and no extensions; every other kind's `names` are file
  names. Every extension and every name appears once; every key MUST be in lower case; parent chains have
  no cycles.
- `thesvg.kinds.json`: `{ "kinds": { "<kind>": "<source slug>" } }`, the image of theSVG for a kind. Every
  kind MUST exist in `kinds.json`; every slug MUST exist in `fill.json` of the pinned clone.
- The role table (`folder`, `file`, `image`, `video`, `audio`, `pdf`, `text` → slug) is the constant
  `ROLE_SLUGS` in `scripts/icon-sets/thesvg.ts`.
- `thesvg.colors.json`: `{ "<slug>": "#rrggbb" }` for every slug used by the table and the roles (and no colour for an unused slug); the
  relative luminance of every colour MUST lie between 0.10 and 0.65 (FR-005).
- `thesvg.source.json`: `{ "repository": "https://github.com/glincker/thesvg", "commit": "<sha>" }`; the
  build fails when the clone's HEAD differs.

## Pack source directory `src-tauri/icon-sets/thesvg/` and `thesvg.zip`

- `icon-theme.json` in the format of spec 053 FR-008: `iconDefinitions` (one id per slug, `iconPath` →
  `icons/<slug>.svg`), `file`, `folder`, `fileExtensions`, `fileNames`, and `x-haex` with `roles`
  (the seven roles plus the five state roles) and `origin` (author "thesvg.org", license "MIT", source
  the repository address). No `light` block (FR-005).
- `icons/<slug>.svg`: the fill-look SVG with `currentColor` replaced by the slug's colour, nothing else
  changed; plus the five `haex-*` state images.
- `LICENSE`: the MIT text of the clone. `NOTICE`: source address, pinned commit, the line that the
  drawings are by thesvg.org and were coloured at build time.

Pack limits of spec 053 hold: at most 2000 images, 256 KiB per image, 5 MiB unpacked, 8 MiB packed.
(About 25 images; each fill-look SVG is 0.5 to 4 KB.)

## Validation rules (build)

1. Every slug named by the mapping, the roles or the colours exists in the source.
2. Every used slug has a colour; every colour is `#rrggbb` and inside the luminance range.
3. An extension or name maps once.
4. Every written SVG passes the pack checks (Rust, `build_bundled_packs`).
5. Two builds from the same commit give identical bytes (SC-003).
