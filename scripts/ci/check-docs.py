#!/usr/bin/env python3
"""Validate the repository's documentation-only CI contract.

Hygiene-only. Checks that required spec documents exist and that local
Markdown links resolve. Deliberately does NOT pin the prose of the spec:
tightening or rewording is the author's call, not the CI's.
"""

from pathlib import Path
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[2]
DOC_ROOT = ROOT / "specs" / "001-frontend-onboarding"

REQUIRED_FILES = (
    DOC_ROOT / "plan.md",
    DOC_ROOT / "research.md",
    DOC_ROOT / "spec.md",
    DOC_ROOT / "tasks.md",
)


def check_required_files(errors: list[str]) -> None:
    for path in REQUIRED_FILES:
        if not path.is_file():
            errors.append(f"missing required document: {path.relative_to(ROOT)}")


def check_local_markdown_links(errors: list[str]) -> None:
    link_pattern = re.compile(r"(?<!!)\[[^]]+\]\(([^)]+)\)")
    documents = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z", "--", "*.md"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split("\0")
    for relative_path in documents:
        document = ROOT / relative_path
        # Git's index may still contain a document deleted in the working tree.
        if not relative_path or not document.is_file():
            continue
        for raw_target in link_pattern.findall(document.read_text()):
            target = raw_target.strip().split(maxsplit=1)[0].strip("<>")
            if not target or re.match(r"^[a-z][a-z0-9+.-]*:", target) or target.startswith("#"):
                continue
            # These links intentionally point at the sibling haex-vault 2.x (Legacy) repository,
            # which is not checked out in this repository's CI workspace.
            if target.startswith("../../../../haex-vault-legacy/"):
                continue
            target_path = (document.parent / target.split("#", 1)[0]).resolve()
            if not target_path.exists():
                errors.append(
                    f"broken local Markdown link in {document.relative_to(ROOT)}: {target}"
                )


def main() -> int:
    errors: list[str] = []
    check_required_files(errors)
    check_local_markdown_links(errors)

    if errors:
        print("Documentation checks failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print("Documentation checks passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
