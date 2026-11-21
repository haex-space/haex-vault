#!/usr/bin/env python3
"""Keep the old name out of the repository (spec 054 FR-004, FR-005).

Searches every file Git tracks, by path and by content (text files only), for the old name in any
spelling and also as a part of a word. What may keep it is listed, with a reason, in
`scripts/ci/old-name-allowlist.txt`; an entry that matches nothing is an error, so the list cannot
outlive its reasons. Contract: specs/054-haex-vault-rename/contracts/old-name-gate.md.

Exit codes: 0 clean, 1 findings or a broken list, 2 not a Git repository.
"""

from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
ALLOWLIST = Path("scripts/ci/old-name-allowlist.txt")
# This file carries the name it looks for; the list allows it by an explicit entry.
OLD_NAME = re.compile("holzi", re.IGNORECASE)
MAX_FINDINGS = 200
MAX_LINE = 200
KINDS = ("path", "pathname", "line")
BINARY_PROBE = 8192


class Entry:
    def __init__(self, number, kind, pattern, reason, until):
        self.number = number
        self.kind = kind
        self.pattern = pattern
        self.reason = reason
        self.until = until
        self.used = False
        self.regex = None


def glob_to_regex(pattern):
    """`**` spans directories, `*` and `?` stay inside one path part."""
    out = []
    i = 0
    while i < len(pattern):
        if pattern.startswith("**", i):
            out.append(".*")
            i += 2
        elif pattern[i] == "*":
            out.append("[^/]*")
            i += 1
        elif pattern[i] == "?":
            out.append("[^/]")
            i += 1
        else:
            out.append(re.escape(pattern[i]))
            i += 1
    return "".join(out)


def read_allowlist(root):
    """The entries, and the problems found while reading them."""
    entries, problems = [], []
    path = root / ALLOWLIST
    if not path.is_file():
        return entries, problems
    shown = ALLOWLIST.as_posix()
    for number, raw in enumerate(path.read_text().splitlines(), start=1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        fields = [part.strip() for part in line.split(" | ")]
        # `kind | pattern | reason |` (a bare trailing pipe) is a list without a reason.
        if line.endswith("|") and len(fields) < 4:
            fields = [part.strip() for part in line.rstrip("|").split(" | ")] + [""]
        kind = fields[0] if fields else ""
        pattern = fields[1] if len(fields) > 1 else ""
        reason = fields[2] if len(fields) > 2 else ""
        until = fields[3] if len(fields) > 3 else ""
        where = f"{shown}:{number}"
        if kind not in KINDS:
            problems.append(f"{where}: unknown kind '{kind}' (use {', '.join(KINDS)})")
            continue
        if not pattern:
            problems.append(f"{where}: empty pattern")
            continue
        if not reason:
            problems.append(f"{where}: no reason for '{pattern}'")
            continue
        entry = Entry(number, kind, pattern, reason, until)
        try:
            if kind == "line":
                entry.regex = re.compile(pattern)
            elif kind == "path":
                entry.regex = re.compile(glob_to_regex(pattern))
            else:
                entry.regex = re.compile("(" + glob_to_regex(pattern) + ")(?=/|$)")
        except re.error as error:
            problems.append(f"{where}: bad pattern '{pattern}' ({error})")
            continue
        entries.append(entry)
    return entries, problems


def tracked_files(root):
    result = subprocess.run(
        ["git", "-C", str(root), "ls-files", "-z"], capture_output=True, check=False
    )
    if result.returncode != 0:
        return None
    return [name for name in result.stdout.decode("utf-8", "replace").split("\0") if name]


def path_findings(path, entries):
    """`None` when the path is clean or allowed (the entry that allowed it is marked used)."""
    if not OLD_NAME.search(path):
        return None
    for entry in entries:
        if entry.kind == "path" and entry.regex.fullmatch(path):
            entry.used = True
            return None
    for entry in entries:
        if entry.kind == "pathname":
            match = entry.regex.match(path)
            if match is None:
                continue
            rest = path[: match.start(1)] + path[match.end(1):]
            if not OLD_NAME.search(rest):
                entry.used = True
                return None
    return path


def line_allowed(line, entries):
    for entry in entries:
        if entry.kind == "line" and entry.regex.search(line):
            entry.used = True
            return True
    return False


def run(root):
    """(exit code, output lines) for the repository at `root`."""
    root = Path(root)
    files = tracked_files(root)
    if files is None:
        return 2, [f"{root} is not a Git repository"]
    entries, problems = read_allowlist(root)
    findings = []
    allowed = 0
    for name in files:
        path_hit = path_findings(name, entries)
        if path_hit is not None:
            findings.append(f"path: {path_hit}")
        elif OLD_NAME.search(name):
            allowed += 1
        file = root / name
        try:
            data = file.read_bytes()
        except OSError:
            continue
        if b"\0" in data[:BINARY_PROBE]:
            continue
        whole_file = any(
            entry.kind == "path" and entry.regex.fullmatch(name) for entry in entries
        )
        for number, line in enumerate(data.decode("utf-8", "replace").splitlines(), start=1):
            if not OLD_NAME.search(line):
                continue
            if whole_file:
                for entry in entries:
                    if entry.kind == "path" and entry.regex.fullmatch(name):
                        entry.used = True
                allowed += 1
            elif line_allowed(line, entries):
                allowed += 1
            else:
                findings.append(f"{name}:{number}: {line.strip()[:MAX_LINE]}")
    for entry in entries:
        if not entry.used:
            problems.append(
                f"{ALLOWLIST.as_posix()}:{entry.number}: unused entry '{entry.pattern}' "
                f"({entry.kind}); remove it"
            )
    output = []
    if findings:
        output.extend(findings[:MAX_FINDINGS])
        shown = min(len(findings), MAX_FINDINGS)
        output.append(f"old name: {len(findings)} findings ({shown} shown)")
        output.append(
            f"Rename it, or list it with a reason in {ALLOWLIST.as_posix()} "
            "(spec 054 contracts/old-name-gate.md)."
        )
    output.extend(problems)
    if findings or problems:
        return 1, output
    hints = sorted({entry.until for entry in entries if entry.until})
    summary = f"old name: clean ({len(files)} files, {allowed} allowed)"
    output.append(summary)
    if hints:
        pending = ", ".join(
            f"{entry.pattern} (until {entry.until})" for entry in entries if entry.until
        )
        output.insert(0, f"entries to remove later: {pending}")
    return 0, output


def main():
    code, output = run(ROOT)
    stream = sys.stdout if code == 0 else sys.stderr
    for line in output:
        print(line, file=stream)
    return code


if __name__ == "__main__":
    sys.exit(main())
