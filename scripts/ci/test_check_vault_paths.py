"""The guard for spec 084 FR-003 finds a hand-built vault path and accepts the resolver."""

import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "check_vault_paths", Path(__file__).with_name("check-vault-paths.py")
)
GUARD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GUARD)


class VaultPathGuardTest(unittest.TestCase):
    def test_the_repository_builds_vault_paths_in_one_place(self):
        self.assertEqual(list(GUARD.offences()), [])

    def test_a_hand_built_path_is_found_and_a_test_or_comment_is_not(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "a.rs").write_text('let p = base.join("chat-files").join(name);\n')
            (root / "b_tests.rs").write_text('let p = base.join("instances");\n')
            (root / "c.rs").write_text('// base.join("instances")\nlet q = base.join("models");\n')
            found = [path.name for path, _, _ in GUARD.offences(root)]
            self.assertEqual(found, ["a.rs"])

    def test_the_vault_folder_is_guarded_too(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "a.rs").write_text('let p = base.join("vaults").join(name);\n')

            found = [path.name for path, _, _ in GUARD.offences(root)]

            self.assertEqual(found, ["a.rs"])

    def test_the_filter_lists_folder_is_guarded_too(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "a.rs").write_text('let p = base.join("filter-lists");\n')

            found = [path.name for path, _, _ in GUARD.offences(root)]

            self.assertEqual(found, ["a.rs"])

    def test_a_multiline_hand_built_path_is_found(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "a.rs").write_text('''let p = base
    .join(
        "instances",
    );
''')

            found = [path.name for path, _, _ in GUARD.offences(root)]

            self.assertEqual(found, ["a.rs"])

    def test_comments_and_unrelated_strings_are_ignored(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "a.rs").write_text('''/* base.join("instances") */
let text = ".join(\\"instances\\")";
let example = r#"base.join("agent-browser")"#;
let path = base.join(
    "chat-files",
);
''')

            found = [path.name for path, _, _ in GUARD.offences(root)]

            self.assertEqual(found, ["a.rs"])

    def test_a_join_using_a_named_path_constant_is_found(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "a.rs").write_text('''const CHAT_FILES_DIR: &str = "chat-files";
let path = base.join(CHAT_FILES_DIR);
''')

            found = [path.name for path, _, _ in GUARD.offences(root)]

            self.assertEqual(found, ["a.rs"])


if __name__ == "__main__":
    unittest.main()
