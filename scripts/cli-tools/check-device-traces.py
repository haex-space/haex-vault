#!/usr/bin/env python3
"""Finds device-wide files that name a vault, a tool or request text (spec 094 SC-009, T043).

    check-device-traces.py <app-local-data> <needle>...

Searches every file below the app's local-data folder except `vaults/` (a vault's own folder may
name itself: it is the vault), including `logs/` and the webview stores (`storage`,
`localstorage`, `WebKitCache`, `cookies`, ...). Each needle is looked for as written, as UTF-16
(webview stores keep strings that way), and percent-encoded (URLs). Prints each match with its
file and exits with 1 when there is one.

A file it cannot read is reported as unreadable, and it does not follow
symbolic links: a link is looked at by name only.
"""

import sys
from pathlib import Path
from urllib.parse import quote

VAULTS = "vaults"
CHUNK = 1 << 20


def spellings(needle):
    forms = {
        "as written": needle.encode("utf-8"),
        "as UTF-16": needle.encode("utf-16-le"),
        "percent-encoded": quote(needle, safe="").encode("ascii"),
    }
    # A form identical to an earlier one is not a second search.
    seen, unique = set(), {}
    for label, form in forms.items():
        if form and form not in seen:
            seen.add(form)
            unique[label] = form
    return unique


def contains(path, forms):
    """The labels of the spellings found in the file, reading it in overlapping chunks."""
    longest = max(len(form) for form in forms.values())
    found, tail = set(), b""
    try:
        with open(path, "rb") as handle:
            while chunk := handle.read(CHUNK):
                window = tail + chunk
                found.update(label for label, form in forms.items() if form in window)
                tail = window[-(longest - 1):] if longest > 1 else b""
    except OSError:
        return {"unreadable"}
    return found


def device_files(root):
    """Every file below `root` outside `vaults/`; links are not followed."""
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root)
        if relative.parts[0] == VAULTS:
            continue
        if path.is_symlink() or path.is_file():
            yield path


def traces(root, needles):
    """(file, needle, spelling) for every match, and the file names that carry a needle."""
    searches = {needle: spellings(needle) for needle in needles}
    hits = []
    for path in device_files(root):
        # A needle in the name of a file or folder is a trace as well.
        name = str(path.relative_to(root))
        for needle in needles:
            if needle in name:
                hits.append((name, needle, "in the file name"))
        if path.is_symlink():
            continue
        for needle, forms in searches.items():
            for label in sorted(contains(path, forms)):
                hits.append((name, needle, label))
    return hits


def main(argv):
    if len(argv) < 3:
        print(__doc__)
        return 2
    root = Path(argv[1])
    if not root.is_dir():
        print(f"check-device-traces: {root} is not a folder", file=sys.stderr)
        return 2
    hits = traces(root, argv[2:])
    for name, needle, label in hits:
        print(f"{name}: {needle!r} ({label})")
    if hits:
        print(f"{len(hits)} trace(s) outside vaults/")
        return 1
    print("no file outside vaults/ names a needle")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
