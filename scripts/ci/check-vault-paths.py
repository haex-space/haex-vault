#!/usr/bin/env python3
"""Spec 084 (FR-003): one place knows where the files of a vault lie.

Fails when Rust code outside `instances/vault_paths.rs` joins one of the folder names that hold
vault files (`instances`, `vaults`, `agent-browser`, `chat-files`, `filter-lists`) onto a path. Tests may name them, since
they check the result on disk.
"""

from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "src-tauri" / "src"
ALLOWED = SOURCE / "instances" / "vault_paths.rs"
PATH_DIRECTORIES = {"instances", "vaults", "agent-browser", "chat-files", "filter-lists"}
JOIN_PATTERN = re.compile(
    r'\.join\(\s*(?:"(?P<literal>instances|vaults|agent-browser|chat-files|filter-lists)"|(?P<constant>[A-Za-z_]\w*))\s*,?\s*\)'
)
PATH_CONSTANT_PATTERN = re.compile(
    r'\b(?:const|static)\s+(?P<name>[A-Za-z_]\w*)\s*:\s*[^=;]+?=\s*'
    r'"(?P<path>instances|vaults|agent-browser|chat-files|filter-lists)"\s*;'
)


def is_test(path: Path) -> bool:
    return path.name.endswith("_tests.rs") or path.name == "test_support.rs"


def mask_non_code(source: str) -> str:
    """Blank Rust comments and unrelated string literals, preserving line numbers."""
    chars = list(source)

    def blank(start: int, end: int, keep: str | None = None) -> None:
        for position in range(start, end):
            if chars[position] not in "\r\n":
                chars[position] = " "
        if keep is not None:
            chars[start : start + len(keep)] = keep

    index = 0
    while index < len(source):
        if source.startswith("//", index):
            end = source.find("\n", index)
            end = len(source) if end == -1 else end
            blank(index, end)
            index = end
            continue

        if source.startswith("/*", index):
            end = index + 2
            depth = 1
            while end < len(source) and depth:
                if source.startswith("/*", end):
                    depth += 1
                    end += 2
                elif source.startswith("*/", end):
                    depth -= 1
                    end += 2
                else:
                    end += 1
            blank(index, end)
            index = end
            continue

        raw_start = index + 2 if source.startswith("br", index) else index + 1
        if source.startswith("br", index) or source.startswith("r", index):
            hashes_end = raw_start
            while hashes_end < len(source) and source[hashes_end] == "#":
                hashes_end += 1
            if hashes_end < len(source) and source[hashes_end] == '"':
                delimiter = '"' + "#" * (hashes_end - raw_start)
                content_start = hashes_end + 1
                content_end = source.find(delimiter, content_start)
                if content_end == -1:
                    content_end = len(source)
                    end = content_end
                else:
                    end = content_end + len(delimiter)
                value = source[content_start:content_end]
                keep = f'"{value}"' if value in PATH_DIRECTORIES else None
                blank(index, end, keep)
                index = end
                continue

        if source[index] == '"':
            end = index + 1
            escaped = False
            while end < len(source):
                char = source[end]
                end += 1
                if escaped:
                    escaped = False
                elif char == "\\":
                    escaped = True
                elif char == '"':
                    break
            value = source[index + 1 : end - 1] if source[end - 1 : end] == '"' else ""
            keep = source[index:end] if value in PATH_DIRECTORIES else None
            blank(index, end, keep)
            index = end
            continue

        index += 1

    return "".join(chars)


def offences(root: Path = SOURCE):
    for path in sorted(root.rglob("*.rs")):
        if path == ALLOWED or is_test(path):
            continue
        # Preserve the path literal while hiding comments and unrelated string contents; the
        # matcher still spans rustfmt-style multiline `.join(...)` calls.
        source = mask_non_code(path.read_text(encoding="utf-8"))
        path_constants = {
            match.group("name")
            for match in PATH_CONSTANT_PATTERN.finditer(source)
        }
        shown = path.relative_to(ROOT) if ROOT in path.parents else path
        for match in JOIN_PATTERN.finditer(source):
            if match.group("literal") is None and match.group("constant") not in path_constants:
                continue
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
