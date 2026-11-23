"""check-containment.py finds what a program writes outside its tree and honours the list."""

import contextlib
import importlib.util
import io
import json
import os
import re
import shutil
import stat
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

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
            '101 openat(AT_FDCWD</tmp>, "cwd-leak", O_WRONLY|O_CREAT, 0600) = 9</tmp/cwd-leak>',
            '102 openat(AT_FDCWD, "resumed", <unfinished ...>',
            '[pid 102] <... openat resumed>O_WRONLY|O_CREAT, 0600) = 10</var/tmp/resumed>',
            '103 bind(3, {sa_family=AF_UNIX, sun_path="/tmp/unlisted.sock"}, 24) = 0',
            '104 openat(AT_FDCWD, "/tree/link/file", O_WRONLY|O_CREAT, 0600) = 11</outside/file>',
            '105 ftruncate(12</tmp/metadata-leak>, 0) = 0',
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
                ("write", "/tmp/cwd-leak"),
                ("write", "/var/tmp/resumed"),
                ("write", "/tmp/unlisted.sock"),
                ("write", "/outside/file"),
                ("write", "/tmp/metadata-leak"),
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
                {"vendor": "*", "pattern": r"/tmp/haex-[0-9a-f]{12}\.sock"},
            ]
        }
        found = [
            ("mkdir", "/tmp/cc-socks-1000"),
            ("write", "/tmp/cc-socks-1000/4242.sock"),
            ("write", "/tmp/haex-0123456789ab.sock"),
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


def catalog_vendor(name):
    catalog = json.loads(CHECK.CATALOG.read_text())
    return next(v for v in catalog["vendors"] if v["vendor"] == name)


class AbacusVendorTest(unittest.TestCase):
    """The vendor `abacusai` of the catalog (spec 096 research.md section 4), no real binary needed."""

    def test_the_vendor_is_known_to_the_catalog_and_expects_exit_zero(self):
        vendor = catalog_vendor("abacusai")
        self.assertEqual(vendor["config_env"], "ABACUSAI_CODING_AGENT_DIR")
        self.assertEqual(CHECK.EXPECTED_EXIT_CODES["abacusai"], {0})
        # Claude and Codex keep the exit codes they had.
        self.assertEqual(CHECK.EXPECTED_EXIT_CODES["claude"], {0, 1})
        self.assertEqual(CHECK.EXPECTED_EXIT_CODES["codex"], {0})

    def test_the_environment_is_built_from_the_vendor_definition_only(self):
        vendor = catalog_vendor("abacusai")
        host = {
            "ABACUS_API_KEY": "host",
            "ABACUSAI_API_KEY": "host",
            "CODELLM_API_KEY": "host",
            "XDG_CONFIG_HOME": "/host/config",
            "PATH": "/usr/bin:/bin",
        }
        with tempfile.TemporaryDirectory() as scratch, mock.patch.dict(os.environ, host, clear=True):
            tree = Path(os.path.realpath(scratch))
            env = CHECK.environment(vendor, tree)
        self.assertEqual(env["HOME"], str(tree / "home"))
        self.assertEqual(env["TMPDIR"], str(tree / "tmp"))
        self.assertEqual(env["ABACUSAI_CODING_AGENT_DIR"], str(tree / "config"))
        self.assertEqual(env["ABACUS_AUTO_UPDATE"], "off")
        self.assertEqual(env["ABACUS_DISABLE_AUTO_MEMORY"], "1")
        self.assertEqual(env["XDG_CONFIG_HOME"], str(tree / "home" / ".config"))
        self.assertEqual(env["XDG_CACHE_HOME"], str(tree / "home" / ".cache"))
        self.assertEqual(env["XDG_DATA_HOME"], str(tree / "home" / ".local" / "share"))
        self.assertEqual(env["XDG_STATE_HOME"], str(tree / "home" / ".local" / "state"))
        for name, value in env.items():
            if name != "PATH":
                self.assertTrue(value.startswith(str(tree)) or name.startswith("ABACUS"), name)
        self.assertNotIn("host", env.values())
        self.assertEqual(
            sorted(env),
            sorted(
                [
                    "PATH", "HOME", "TMPDIR", "ABACUSAI_CODING_AGENT_DIR", "ABACUS_AUTO_UPDATE",
                    "ABACUS_DISABLE_AUTO_MEMORY", *CHECK.XDG_BELOW_HOME,
                ]
            ),
        )

    def test_the_base_variables_of_the_other_vendors_are_unchanged(self):
        with tempfile.TemporaryDirectory() as scratch:
            tree = Path(os.path.realpath(scratch))
            claude = CHECK.environment(catalog_vendor("claude"), tree)
            codex = CHECK.environment(catalog_vendor("codex"), tree)
        self.assertEqual(claude["CLAUDE_CONFIG_DIR"], str(tree / "config"))
        self.assertEqual(claude["CLAUDE_CODE_TMPDIR"], str(tree / "tmp"))
        self.assertEqual(claude["DISABLE_UPDATES"], "1")
        self.assertEqual(codex["CODEX_HOME"], str(tree / "config"))
        self.assertNotIn("ABACUSAI_CODING_AGENT_DIR", claude)

    def test_the_trace_of_the_unauthenticated_series_stays_in_the_tree(self):
        tree = "/scratch/tree"
        lines = [
            f'1 openat(AT_FDCWD, "{tree}/home/.abacusai/logs/t-1.log", O_WRONLY|O_CREAT|O_APPEND, 0644) = 3<{tree}/home/.abacusai/logs/t-1.log>',
            f'1 mkdir("{tree}/home/.abacusai/cli", 0777) = 0',
            f'1 openat(AT_FDCWD, "{tree}/home/.abacusai/cli/9.pid", O_WRONLY|O_CREAT|O_TRUNC, 0666) = 4<{tree}/home/.abacusai/cli/9.pid>',
            f'1 unlink("{tree}/home/.abacusai/cli/9.pid") = 0',
            f'1 mkdir("{tree}/home/.config/abacusai-cli-nodejs", 0777) = 0',
            f'1 openat(AT_FDCWD, "{tree}/tmp/.bun-1000-abc.node", O_WRONLY|O_CREAT|O_TRUNC, 0755) = 5<{tree}/tmp/.bun-1000-abc.node>',
            '1 openat(AT_FDCWD, "/usr/bin/ldd", O_RDONLY|O_CLOEXEC) = 6</usr/bin/ldd>',
        ]
        events, unresolved = CHECK.events_of(lines, f"{tree}/run")
        self.assertEqual(unresolved, [])
        self.assertEqual(CHECK.outside(events, tree), [])

    def test_the_traces_of_an_unset_tmpdir_and_home_are_offences_no_residual_covers(self):
        shipped = json.loads(Path(__file__).with_name("allowed-residuals.json").read_text())
        patterns = CHECK.allowed_patterns(shipped, "abacusai")
        found = [
            ("write", "/tmp/.bun-1000-abc.node"),
            ("mkdir", "/home/someone/.abacusai/logs"),
            ("mkdir", "/home/someone/.config/abacusai-cli-nodejs"),
        ]
        self.assertEqual(CHECK.offences(found, patterns), sorted(found))
        # The claude socket entry is not an Abacus residual.
        self.assertEqual(
            CHECK.offences([("write", "/tmp/cc-socks-1000/1.sock")], patterns),
            [("write", "/tmp/cc-socks-1000/1.sock")],
        )


@unittest.skipUnless(shutil.which("strace"), "strace is not installed")
class AbacusMeasuredRunTest(unittest.TestCase):
    """A stub that writes where the real tool writes (research.md section 4)."""

    STUB = """#!/bin/sh
[ "$1" = "--version" ] || exit 9
[ "$ABACUS_AUTO_UPDATE" = off ] || exit 3
[ "$ABACUS_DISABLE_AUTO_MEMORY" = 1 ] || exit 3
[ -n "$ABACUSAI_CODING_AGENT_DIR" ] || exit 3
[ -z "$ABACUS_API_KEY$ABACUSAI_API_KEY$CODELLM_API_KEY" ] || exit 4
mkdir -p "$HOME/.abacusai/logs" "$HOME/.abacusai/cli" "$XDG_CONFIG_HOME/abacusai-cli-nodejs"
echo x > "$HOME/.abacusai/logs/t-1.log"
echo 1 > "$HOME/.abacusai/cli/1.pid"; rm "$HOME/.abacusai/cli/1.pid"
echo x > "$TMPDIR/.bun-1000-abc.node"
%s
echo 2.10.23
exit %d
"""

    def stub(self, directory, extra="", code=0):
        path = Path(directory) / "abacusai"
        path.write_text(self.STUB % (extra, code))
        path.chmod(path.stat().st_mode | stat.S_IEXEC)
        return path

    def measure(self, stub, vendor):
        host = {"ABACUS_API_KEY": "host", "ABACUSAI_API_KEY": "host", "CODELLM_API_KEY": "host"}
        with mock.patch.dict(os.environ, host):
            return CHECK.measure(vendor, stub, ["--version"], 30)

    def test_the_unauthenticated_series_has_no_residue(self):
        vendor = catalog_vendor("abacusai")
        with tempfile.TemporaryDirectory() as directory:
            found, _, code = self.measure(self.stub(directory), vendor)
        self.assertEqual(code, 0)
        self.assertEqual(found, [])

    def test_a_write_to_the_real_tmp_is_found_and_not_listed(self):
        vendor = catalog_vendor("abacusai")
        shipped = json.loads(Path(__file__).with_name("allowed-residuals.json").read_text())
        with tempfile.TemporaryDirectory() as directory:
            leak = Path(os.path.realpath(directory)) / ".bun-leak.node"
            found, _, code = self.measure(self.stub(directory, f'echo x > "{leak}"'), vendor)
        self.assertEqual(code, 0)
        self.assertEqual(
            CHECK.offences(found, CHECK.allowed_patterns(shipped, "abacusai")), [("write", str(leak))]
        )

    def run_main(self, stub, vendor="abacusai"):
        argv = ["check-containment.py", "--vendor", vendor, "--program", str(stub), "--", "--version"]
        with mock.patch.object(sys, "argv", argv), contextlib.redirect_stdout(io.StringIO()) as out:
            return CHECK.main(), out.getvalue()

    def test_main_accepts_exit_zero_and_refuses_another_exit(self):
        with tempfile.TemporaryDirectory() as directory:
            result, output = self.run_main(self.stub(directory))
            self.assertEqual(result, 0, output)
            self.assertIn("exit code of the tool: 0", output)
            result, output = self.run_main(self.stub(directory, code=1))
            self.assertEqual(result, 1, output)
            self.assertIn("must not be added", output)


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
