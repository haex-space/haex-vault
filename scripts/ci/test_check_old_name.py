"""The gate for the old name (spec 054 FR-004, FR-005) finds it in tracked files and paths, honours
the allow list and reports a broken list."""

import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("check-old-name.py")
SPEC = importlib.util.spec_from_file_location("check_old_name", SCRIPT)
GATE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GATE)

NAME = "holzi"
ALLOWLIST = Path("scripts/ci/old-name-allowlist.txt")
# A list that names the old name is itself a file with a hit; the real list carries this entry too.
ALLOW_SELF = "path | scripts/ci/old-name-allowlist.txt | the list names it\n"


def make_repo(directory, files, allow=None, track=True):
    """A git repository in `directory` holding `files` (path -> text or bytes)."""
    root = Path(directory)
    subprocess.run(["git", "init", "--quiet", str(root)], check=True)
    for relative, content in files.items():
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(content, bytes):
            path.write_bytes(content)
        else:
            path.write_text(content)
    if allow is not None:
        (root / ALLOWLIST).parent.mkdir(parents=True, exist_ok=True)
        (root / ALLOWLIST).write_text(allow)
    if track:
        subprocess.run(["git", "add", "-A"], cwd=root, check=True)
    return root


class OldNameGateTest(unittest.TestCase):
    def check(self, files, allow=None, track=True):
        with tempfile.TemporaryDirectory() as directory:
            root = make_repo(directory, files, allow, track)
            return GATE.run(root)

    def test_the_repository_itself_is_clean(self):
        root = Path(__file__).resolve().parents[2]
        code, output = GATE.run(root)
        self.assertEqual(code, 0, "\n".join(output))
        self.assertIn("old name: clean", output[-1])

    def test_a_clean_repository_passes(self):
        code, output = self.check({"a.txt": "nothing here\n"})
        self.assertEqual(code, 0, output)
        self.assertEqual(output, ["old name: clean (1 files, 0 allowed)"])

    def test_the_name_in_a_text_is_found_in_every_spelling(self):
        code, output = self.check(
            {
                "lower.md": f"first\nthe {NAME} app\n",
                "upper.md": f"{NAME.upper()}_X = 1\n",
                "part.rs": f"struct {NAME.capitalize()}Error;\n",
            }
        )
        self.assertEqual(code, 1)
        text = "\n".join(output)
        self.assertIn(f"lower.md:2: the {NAME} app", text)
        self.assertIn(f"upper.md:1: {NAME.upper()}_X = 1", text)
        self.assertIn(f"part.rs:1: struct {NAME.capitalize()}Error;", text)

    def test_the_name_in_a_file_name_is_found_and_a_pathname_entry_lets_only_the_name_pass(self):
        files = {f"dir-{NAME}/note.md": "clean\n"}
        code, output = self.check(files)
        self.assertEqual(code, 1)
        self.assertIn(f"path: dir-{NAME}/note.md", "\n".join(output))

        allow = ALLOW_SELF + f"pathname | dir-{NAME} | the directory keeps its name\n"
        code, output = self.check(files, allow)
        self.assertEqual(code, 0, output)

        files[f"dir-{NAME}/note.md"] = f"the {NAME} app\n"
        code, output = self.check(files, allow)
        self.assertEqual(code, 1)
        self.assertIn(f"dir-{NAME}/note.md:1:", "\n".join(output))

    def test_a_pathname_entry_does_not_cover_a_second_name_in_the_same_path(self):
        files = {f"dir-{NAME}/{NAME}-note.md": "clean\n"}
        allow = ALLOW_SELF + f"pathname | dir-{NAME} | the directory keeps its name\n"
        code, output = self.check(files, allow)
        self.assertEqual(code, 1)
        self.assertIn(f"path: dir-{NAME}/{NAME}-note.md", "\n".join(output))

    def test_a_binary_file_is_skipped_by_content_and_checked_by_path(self):
        data = b"\x00\x01" + NAME.encode() + b"\x00"
        code, output = self.check({"image.bin": data})
        self.assertEqual(code, 0, output)

        code, output = self.check({f"{NAME}.bin": data})
        self.assertEqual(code, 1)
        self.assertIn(f"path: {NAME}.bin", "\n".join(output))

    def test_a_path_entry_covers_a_whole_file_and_a_line_entry_only_its_line(self):
        files = {
            "gate/list.txt": f"{NAME} twice {NAME}\nmore {NAME}\n",
            "other.md": f"said once: {NAME}\nand again: {NAME} here\n",
        }
        allow = (
            ALLOW_SELF
            + "path | gate/** | the gate names it\n"
            f"line | ^said once: {NAME}$ | quoted on purpose\n"
        )
        code, output = self.check(files, allow)
        self.assertEqual(code, 1)
        text = "\n".join(output)
        self.assertNotIn("gate/list.txt", text)
        self.assertNotIn("other.md:1:", text)
        self.assertIn(f"other.md:2: and again: {NAME} here", text)

    def test_a_line_entry_can_allow_the_name_inside_a_longer_token_only(self):
        keep = "keep-" + NAME
        files = {"a.md": f"see {keep}/spec.md\nsee {keep}/spec.md and {NAME}\n"}
        allow = ALLOW_SELF + f"line | ^(?:{keep}|(?!{NAME}).)*$ | the path stays | S10\n"
        code, output = self.check(files, allow)
        self.assertEqual(code, 1)
        text = "\n".join(output)
        self.assertNotIn("a.md:1:", text)
        self.assertIn("a.md:2:", text)

    def test_a_broken_unused_or_incomplete_entry_fails_with_its_line(self):
        cases = {
            "no reason": "path | a.txt |\n",
            "extra field": "path | a.txt | because | S10 | ignored\n",
            "empty pattern": "path |  | because\n",
            "unknown kind": "glob | a.txt | because\n",
            "bad regex": "line | ( | because\n",
            "unused": "path | nowhere/** | because\n",
        }
        for label, allow in cases.items():
            with self.subTest(label):
                code, output = self.check({"a.txt": f"{NAME}\n"}, allow)
                self.assertEqual(code, 1, output)
                self.assertIn("old-name-allowlist.txt:1", "\n".join(output))

    def test_comments_and_blank_lines_in_the_list_are_ignored(self):
        allow = "# a comment\n\npath | a.txt | because\n"
        code, output = self.check({"a.txt": f"{NAME}\n"}, allow)
        self.assertEqual(code, 0, output)
        self.assertEqual(output[-1], "old name: clean (2 files, 1 allowed)")

    def test_the_until_field_shows_in_the_summary_only_as_a_hint(self):
        allow = "path | a.txt | because | S10\n"
        code, output = self.check({"a.txt": f"{NAME}\n"}, allow)
        self.assertEqual(code, 0, output)
        self.assertIn("S10", "\n".join(output))

    def test_a_file_that_git_does_not_track_is_ignored(self):
        code, output = self.check({"a.txt": f"{NAME}\n"}, track=False)
        self.assertEqual(code, 0, output)

    def test_a_long_list_stops_after_two_hundred_findings_and_names_the_total(self):
        files = {f"f{n}.txt": f"{NAME}\n" for n in range(230)}
        code, output = self.check(files)
        self.assertEqual(code, 1)
        self.assertEqual(sum(1 for line in output if line.startswith("f")), 200)
        self.assertIn("230", output[-2])

    def test_a_long_line_is_cut(self):
        code, output = self.check({"a.txt": f"{NAME} " + "x" * 500 + "\n"})
        self.assertEqual(code, 1)
        shown = [line for line in output if line.startswith("a.txt:1:")][0]
        self.assertLessEqual(len(shown.split(": ", 1)[1]), 200)

    def test_outside_a_git_repository_the_exit_code_is_two(self):
        with tempfile.TemporaryDirectory() as directory:
            script = Path(directory) / "scripts" / "ci" / "check-old-name.py"
            script.parent.mkdir(parents=True)
            script.write_text(SCRIPT.read_text())
            result = subprocess.run(
                [sys.executable, str(script)], capture_output=True, text=True
            )
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
