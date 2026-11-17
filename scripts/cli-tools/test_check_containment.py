"""check-containment.py finds what a program writes outside its tree and honours the list."""

import importlib.util
import os
import re
import shutil
import stat
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "check_containment", Path(__file__).with_name("check-containment.py")
)
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)

FAKE_VENDOR = {"vendor": "fake", "config_env": "FAKE_CONFIG", "tmp_env": [], "env_set": []}


class TraceTest(unittest.TestCase):
    def test_a_trace_is_read_with_its_directory_handles(self):
        lines = [
            '100 openat(AT_FDCWD, "/etc/hosts", O_RDONLY|O_CLOEXEC) = 3</etc/hosts>',
            '100 openat(AT_FDCWD, "state.db", O_WRONLY|O_CREAT|O_TRUNC, 0644) = 4</run/state.db>',
            '[pid 101] openat(5</home/h/.cache>, "index", O_RDWR|O_CREAT, 0600) = 6</home/h/.cache/index>',
            '101 mkdir("/tmp/outside", 0700) = 0',
            '101 rename("/tmp/outside/a", "/tmp/outside/b") = 0',
            '101 unlink("/tmp/outside/b") = 0',
            '101 openat(AT_FDCWD, "/tmp/never", O_WRONLY|O_CREAT, 0644) = -1 EACCES (Permission denied)',
            '102 mkdirat(7, "newroot", 0755) = 0',
        ]
        events, unresolved = CHECK.events_of(lines, "/run")
        self.assertEqual(
            events,
            [
                ("write", "/run/state.db"),
                ("write", "/home/h/.cache/index"),
                ("mkdir", "/tmp/outside"),
                ("remove", "/tmp/outside/a"),
                ("write", "/tmp/outside/b"),
                ("remove", "/tmp/outside/b"),
            ],
        )
        self.assertEqual(unresolved, [("mkdir", '"newroot"')])

    def test_what_lies_in_the_tree_or_on_a_kernel_path_is_not_outside(self):
        events = [
            ("write", "/scratch/tree/home/x"),
            ("write", "/dev/null"),
            ("write", "/proc/self/oom_score_adj"),
            ("write", "/tmp/leak"),
        ]
        self.assertEqual(CHECK.outside(events, "/scratch/tree"), [("write", "/tmp/leak")])

    def test_a_listed_residual_passes_and_anything_else_is_an_offence(self):
        allowed = {
            "residuals": [
                {"vendor": "claude", "pattern": r"/tmp/cc-socks-[0-9]+(/[0-9]+\.sock)?"},
                {"vendor": "*", "pattern": r"/tmp/holzi-[0-9a-f]{12}\.sock"},
            ]
        }
        found = [
            ("mkdir", "/tmp/cc-socks-1000"),
            ("write", "/tmp/cc-socks-1000/4242.sock"),
            ("write", "/tmp/holzi-0123456789ab.sock"),
            ("write", "/home/someone/.claude.json"),
        ]
        self.assertEqual(
            CHECK.offences(found, CHECK.allowed_patterns(allowed, "claude")),
            [("write", "/home/someone/.claude.json")],
        )
        # The claude entry does not apply to another vendor.
        self.assertIn(
            ("write", "/tmp/cc-socks-1000/4242.sock"),
            CHECK.offences(found, CHECK.allowed_patterns(allowed, "codex")),
        )

    def test_the_shipped_list_names_exactly_the_two_accepted_residuals(self):
        import json

        shipped = json.loads(Path(__file__).with_name("allowed-residuals.json").read_text())
        self.assertEqual(
            sorted(entry["id"] for entry in shipped["residuals"]),
            ["approval-socket-fallback", "claude-inbox-socket"],
        )
        for entry in shipped["residuals"]:
            re.compile(entry["pattern"])


@unittest.skipUnless(shutil.which("strace"), "strace is not installed")
class MeasuredRunTest(unittest.TestCase):
    def program(self, directory, body):
        path = Path(directory) / "tool"
        path.write_text(f"#!/bin/sh\n{body}\n")
        path.chmod(path.stat().st_mode | stat.S_IEXEC)
        return path

    def test_a_program_that_stays_in_its_home_has_no_residue(self):
        with tempfile.TemporaryDirectory() as directory:
            tool = self.program(
                directory,
                'echo x > "$HOME/a"; mkdir "$TMPDIR/d"; echo x > "$FAKE_CONFIG/c"; rm "$HOME/a"',
            )
            found, _, code = CHECK.measure(FAKE_VENDOR, tool, [], 30)
        self.assertEqual(code, 0)
        self.assertEqual(found, [])

    def test_a_write_outside_is_found_and_the_list_decides(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(os.path.realpath(directory)) / "leak"
            tool = self.program(directory, f'echo x > "{target}"')
            found, _, code = CHECK.measure(FAKE_VENDOR, tool, [], 30)
            self.assertEqual(code, 0)
            self.assertIn(("write", str(target)), found)
            self.assertEqual(CHECK.offences(found, []), sorted(set(found)))
            listed = [re.compile(re.escape(str(target)))]
            self.assertEqual(CHECK.offences(found, listed), [])


if __name__ == "__main__":
    unittest.main()
