# Quickstart: theSVG icon set

Validation guide for [spec.md](./spec.md). Run in the worktree, inside the nix devShell
(`nix develop --command …`). The clone path is an argument and is never written into the repository.

## 1. Build

```text
git clone https://github.com/glincker/thesvg.git <dir>   # check out the commit named in thesvg.source.json
node scripts/icon-sets/prepare.ts thesvg <dir>
```

Expected: the log names the icons, the extensions and names, the colours; no error. Run it twice;
`git status` shows no change after the second run (US3, SC-003).

## 2. Build refuses what it should (US3)

- `pnpm check:file-icons`: colour range, the kinds mapping, the sets' tables and missing-slug cases pass.
- Rename one slug in a copy of `thesvg.kinds.json` to a missing one: the build stops and names it.
- Check out another commit in the clone: the build stops and names both commits.

## 3. Rust checks

```text
nix develop --command cargo test -j 4 files::icons storage::preferences_commands
```

Expected: the pack opens and passes the checks, equals what its source directory packs to; `rs`, `zip`,
`pdf`, `json`, `png` and the roles resolve to images in both schemes; an unknown extension gets the
generic file image; the five states resolve to the `haex-*` images; the list names four sets.

## 4. In the app (US1, US2)

1. Settings → Appearance → icon set for files: Lucide, Phosphor, Material, theSVG, each with preview,
   author and license; Material is the default.
2. Choose theSVG: a folder with `a.png`, `a.mp4`, `a.mp3`, `a.pdf`, `a.txt`, `a.rs`, `a.zip` and a
   subfolder shows coloured images of their kind; an encrypted folder shows the green state image.
3. Switch the colour scheme: every image stays readable.
4. Settings → Licenses: "theSVG" with author, MIT, source and the full text.
5. A second device gets the choice (sync); an Android phone shows the same images.

## 5. E2E

`files-icon-sets` (the list has four sets). e2e runs only on main (CI); check the result there after the
merge.
