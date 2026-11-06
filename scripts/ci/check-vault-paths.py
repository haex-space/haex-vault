#!/usr/bin/env python3
"""Spec 084 (FR-003): one place knows where the files of a vault lie.

Fails when Rust code outside `instances/vault_paths.rs` joins one of the folder names that hold
vault files (`instances`, `agent-browser`, `chat-files`) onto a path. Tests may name them, since
they check the result on disk.
"""

from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "src-tauri" / "src"
ALLOWED = SOURCE / "instances" / "vault_paths.rs"
PATTERN = re.compile(r'\.join\(\s*"(instances|agent-browser|chat-files)"\s*,?\s*\)')


def is_test(path: Path) -> bool:
    return path.name.endswith("_tests.rs") or path.name == "test_support.rs"


def offences(root: Path = SOURCE):
    for path in sorted(root.rglob("*.rs")):
        if path == ALLOWED or is_test(path):
            continue
        # Keep matching across line breaks so rustfmt-style multiline `.join(...)` calls are
        # covered too. Blank standalone line comments so a commented example is not reported.
        source = "\n".join(
            "" if line.lstrip().startswith("//") else line
            for line in path.read_text(encoding="utf-8").splitlines()
        )
        shown = path.relative_to(ROOT) if ROOT in path.parents else path
        for match in PATTERN.finditer(source):
            number = source.count("\n", 0, match.start()) + 1
            line = source.splitlines()[number - 1].strip()
            yield shown, number, line


def main() -> int:
    found = list(offences())
    for path, number, line in found:
        print(f"{path}:{number}: builds a vault path by itself: {line}", file=sys.stderr)
    if found:
        print("Use instances::vault_paths::VaultPaths (spec 084 FR-003).", file=sys.stderr)
        return 1
    print("Vault paths are built in one place.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
