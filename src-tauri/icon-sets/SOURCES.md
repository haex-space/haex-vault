# Bundled file icon sets: sources

The source directories `phosphor/` and `material/` are written by
`scripts/icon-sets/prepare.ts` (spec 053, research.md R6) from the pinned
sources below. Do not edit them by hand; change the script or the selection
files and run it again. The script takes the source paths as arguments, so no
local path is recorded anywhere.

```text
node scripts/icon-sets/prepare.ts phosphor <clone of phosphor-icons/core>
node scripts/icon-sets/prepare.ts material <unpacked VSIX> <Apache-2.0 license text>
```

## Phosphor (`phosphor/`)

- Source: https://github.com/phosphor-icons/core
- Version: 2.1.1 (`package.json` of the clone; weight `assets/regular`)
- Commit: `2b75f3ad12b420c9504ef05df8d2564a28f8500e`
- License: `MIT` (Copyright (c) 2023 Phosphor Icons; text in `phosphor/LICENSE`)
- Selection rule: exactly the icons listed in `phosphor.icons.txt` (the
  file-type icons `file-*`, `file`, `folder`, `folder-simple`, `folder-lock`,
  `folder-simple-lock`, `folder-open`; never a `*-logo` icon), and the
  extensions of `phosphor.mapping.json` (hand-written, checked against the list).
- Light and dark: every image is written twice, `<name>.svg` with light strokes
  (`#c9c9c9`, main part of the manifest, dark scheme) and `<name>.light.svg`
  with dark strokes (`#4a4a4a`, block `light`, light scheme).
- Convention for `-light` ids: the dark-scheme id is `<name>`, the light-scheme
  id is `<name>-light`. `x-holzi.roles` names only the main ids; in the light
  scheme the loader takes `<id>-light` when `iconDefinitions` has it, else the
  role's own id.
- ZIP size: 39768 bytes.

## Material Icon Theme (`material/`)

- Source: https://github.com/material-extensions/vscode-material-icon-theme,
  the released VSIX of `PKief.material-icon-theme` (the Git repository is not
  enough: `file.svg`, `folder.svg` and `folder-open.svg` only exist after the
  build)
- Version: 5.39.0 (`extension/package.json` of the VSIX)
- Checksum: SHA-256 of the VSIX file `f3fda4e1d6095d942c92467c1ad6f322a74c62160f7d29741058e4d726922fbd`
- License: `MIT AND Apache-2.0` (MIT for the theme, Copyright (c) 2025 Material
  Extensions, in `material/LICENSE`; Apache-2.0 for icons from Material Design
  Icons (Pictogrammers) and Material Symbols (Google), in
  `material/LICENSE-APACHE-2.0` with `material/NOTICE`; the logos and marks
  shown in some icons belong to their owners and only mark the file type)
- Selection rule: all 1251 images of `extension/icons/` and the manifest
  `extension/dist/material-icons.json` without the parts holzi ignores
  (`folderNamesExpanded`, `rootFolder*`, `rootFolderNamesExpanded`, `languageIds`,
  `highContrast`, `hidesExplorerArrows`, `folderExpanded`); `./../icons/x.svg`
  becomes `icons/x.svg`.
- Light and dark: the theme brings its own `*_light.svg` images and the block
  `light`; both stay as they are. The `-light` id convention above does not
  apply (the theme has no `-light` ids).
- ZIP size: 716455 bytes.

## Lucide (no pack)

- Source: https://github.com/lucide-icons/lucide
- Commit: `d672fc43dbf60962d9d829e35bc3b3e5ed89a178` (the license text in `lucide.LICENSE` is the file `LICENSE` of this commit)
- License: `ISC AND MIT` (ISC for Lucide, MIT for the icons derived from Feather)
- Lucide is the standard set and the fallback of every role another set does not cover. It has no
  images in a pack: holzi draws its symbols with the Iconify icons of the app (`lucide:*`), in the
  colour of the text. `lucide.LICENSE` is the text for the page "Lizenzen".
