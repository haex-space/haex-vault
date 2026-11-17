#!/usr/bin/env python3
"""Measures what a delegate tool writes outside the vault (spec 094 FR-021, research.md section 3).

    check-containment.py --vendor claude --program ./claude -- -p ping

The program runs in an empty tree under `strace -f -y`, with an environment of only `PATH` plus
what holzi's base layer sets for that vendor (`HOME`, `TMPDIR`, the vendor's configuration and
temp variables, the fixed variables of the catalog). Every path the tool created, wrote, renamed
or removed is listed; the ones outside the tree must match `allowed-residuals.json`, otherwise
the script exits with 1 and the version must not enter the tested-version list.

Linux only (needs `strace`). A relative path whose directory handle strace cannot name is
reported as unresolved and does not count as an offence: it has no parent that can be placed
outside the tree (the sandbox helper of Codex makes two such directories).
"""

import argparse
import json
import os
import re
import signal
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
CATALOG = HERE.parent.parent / "src-tauri" / "src" / "cli_tools" / "catalog.json"
ALLOWED = HERE / "allowed-residuals.json"

# What a path below these is not a file the tool keeps: terminals, kernel interfaces.
IGNORED_PREFIXES = ("/dev/", "/proc/", "/sys/")

TRACED = (
    "execve,execveat,openat,open,creat,mkdir,mkdirat,rename,renameat,renameat2,unlink,unlinkat,rmdir,"
    "symlink,symlinkat,link,linkat,truncate,ftruncate,fallocate,chmod,fchmod,fchmodat,"
    "chown,fchown,fchownat,utimensat,futimesat,fchmodat2,mknod,mknodat,bind"
)
WRITE_FLAGS = ("O_WRONLY", "O_RDWR", "O_CREAT", "O_TRUNC", "O_APPEND")

LINE = re.compile(r"^(?:\[pid\s+\d+\]\s+|\d+\s+)?(?P<call>\w+)\((?P<args>.*)\)\s+=\s+(?P<ret>-?\d+|\?)(?P<result>.*)$")
# A quoted path, or a directory handle strace named with -y: `5</some/dir>`.
STRING = r'"((?:[^"\\]|\\.)*)"'
DIRFD = re.compile(r"^(?P<fd>AT_FDCWD|\d+)(?:<(?P<path>[^>]*)>)?$")


def unquote(text):
    return text.encode().decode("unicode_escape")


def split_args(args):
    """Splits a strace argument list at top-level commas (quotes and brackets respected)."""
    parts, depth, quoted, escaped, current = [], 0, False, False, []
    for char in args:
        if quoted:
            current.append(char)
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                quoted = False
            continue
        if char == '"':
            quoted = True
        elif char in "([{<":
            depth += 1
        elif char in ")]}>":
            depth -= 1
        elif char == "," and depth == 0:
            parts.append("".join(current).strip())
            current = []
            continue
        current.append(char)
    parts.append("".join(current).strip())
    return parts


def place(directory, text, cwd):
    """The absolute path of `text` below the directory argument, or None if it cannot be placed."""
    path = unquote(text.strip()[1:-1]) if text.strip().startswith('"') else None
    if path is None:
        return None
    if path.startswith("/"):
        return os.path.normpath(path)
    match = DIRFD.match(directory)
    if match is None:
        return None
    if match.group("path"):
        return os.path.normpath(os.path.join(match.group("path"), path))
    if match.group("fd") == "AT_FDCWD":
        return os.path.normpath(os.path.join(cwd, path))
    return None


def events_of(lines, cwd):
    """The (operation, path) pairs a trace shows as done, and the relative paths it cannot place."""
    events, unresolved = [], []

    def add(op, directory, argument):
        path = place(directory, argument, cwd)
        if path is None:
            unresolved.append((op, argument))
        else:
            events.append((op, path))

    pending, complete = {}, []
    for line in lines:
        prefix = re.match(r"^(?:\[pid\s+(?P<bracket>\d+)\]\s+|(?P<plain>\d+)\s+)?(?P<body>.*)$", line.strip())
        pid = (prefix.group("bracket") or prefix.group("plain") or "main") if prefix else "main"
        body = prefix.group("body") if prefix else line.strip()
        unfinished = body.find("<unfinished ...>")
        resumed = re.match(r"<\.\.\.\s+\w+\s+resumed>(.*)", body)
        if unfinished >= 0:
            pending[pid] = body[:unfinished].rstrip()
            continue
        if resumed:
            start = pending.pop(pid, None)
            if start is None:
                continue
            # strace resumes after the arguments already printed in the unfinished record.
            body = start + resumed.group(1)
        complete.append(body)

    for line in complete:
        found = LINE.match(line.strip())
        if found is None or found.group("ret") in ("-1", "?"):
            continue
        call, args = found.group("call"), split_args(found.group("args"))
        if call in ("openat", "open", "creat") and len(args) >= 2:
            if call == "openat":
                directory, argument, flags = args[0], args[1], args[2] if len(args) > 2 else ""
            else:
                directory, argument = "AT_FDCWD", args[0]
                flags = "O_CREAT|O_WRONLY|O_TRUNC" if call == "creat" else (args[1] if len(args) > 1 else "")
            if call == "creat" or any(flag in flags for flag in WRITE_FLAGS):
                descriptor = re.search(r"<([^<>]*)>\s*$", found.group("result"))
                if descriptor:
                    events.append(("write", os.path.normpath(descriptor.group(1))))
                else:
                    add("write", directory, argument)
        elif call == "mkdir" and args:
            add("mkdir", "AT_FDCWD", args[0])
        elif call == "mkdirat" and len(args) >= 2:
            add("mkdir", args[0], args[1])
        elif call in ("rename", "link", "symlink") and len(args) >= 2:
            if call == "rename":
                add("remove", "AT_FDCWD", args[0])
            add("write", "AT_FDCWD", args[1])
        elif call in ("renameat", "renameat2", "linkat") and len(args) >= 4:
            if call != "linkat":
                add("remove", args[0], args[1])
            add("write", args[2], args[3])
        elif call == "symlinkat" and len(args) >= 3:
            add("write", args[1], args[2])
        elif call in ("unlink", "rmdir") and args:
            add("remove", "AT_FDCWD", args[0])
        elif call == "unlinkat" and len(args) >= 2:
            add("remove", args[0], args[1])
        elif call == "truncate" and args:
            add("write", "AT_FDCWD", args[0])
        elif call in ("ftruncate", "fallocate", "fchmod", "fchown") and args:
            descriptor = re.match(r"\d+<([^>]*)>", args[0])
            if descriptor:
                events.append(("write", os.path.normpath(descriptor.group(1))))
            else:
                unresolved.append(("write", args[0]))
        elif call in ("chmod", "chown", "mknod") and args:
            add("write", "AT_FDCWD", args[0])
        elif call in (
            "fchmodat", "fchmodat2", "fchownat", "mknodat", "utimensat", "futimesat"
        ) and len(args) >= 2:
            add("write", args[0], args[1])
        elif call == "bind" and len(args) >= 2 and "AF_UNIX" in args[1]:
            socket_path = re.search(r'sun_path="((?:[^"\\]|\\.)*)"', args[1])
            if socket_path and not socket_path.group(1).startswith("@"):
                add("write", "AT_FDCWD", f'"{socket_path.group(1)}"')
    return events, unresolved


def outside(events, tree):
    """The events that are not below `tree` and not on a kernel or terminal path."""
    root = os.path.normpath(str(tree))
    kept = []
    for op, path in events:
        if path == root or path.startswith(root + os.sep):
            continue
        if path.startswith(IGNORED_PREFIXES):
            continue
        kept.append((op, path))
    return kept


def allowed_patterns(allowed, vendor):
    return [
        re.compile(entry["pattern"])
        for entry in allowed["residuals"]
        if entry["vendor"] in (vendor, "*")
    ]


def offences(found, patterns):
    return sorted({(op, path) for op, path in found if not any(p.fullmatch(path) for p in patterns)})


def environment(vendor, tree):
    """The environment holzi's base layer gives the tool: `PATH` and what is set for the vendor."""
    home, tmp, config = tree / "home", tree / "tmp", tree / "config"
    for folder in (home, tmp, config):
        folder.mkdir(parents=True, exist_ok=True)
    env = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "HOME": str(home), "TMPDIR": str(tmp)}
    env[vendor["config_env"]] = str(config)
    for name in vendor.get("tmp_env", []):
        env[name] = str(tmp)
    for pair in vendor.get("env_set", []):
        env[pair["name"]] = pair["value"]
    return env


def measure(vendor, program, extra_args, timeout):
    """Runs the program under strace; returns (outside events, unresolved, exit code or None)."""
    if shutil.which("strace") is None:
        sys.exit("check-containment: strace is not installed")
    with tempfile.TemporaryDirectory(prefix="holzi-contain-") as scratch:
        tree = Path(os.path.realpath(scratch)) / "tree"
        tree.mkdir()
        run = tree / "run"
        run.mkdir()
        trace = Path(scratch) / "trace.txt"
        command = [
            "strace", "--kill-on-exit", "-f", "-y", "-qq", "-s", "512", "-e", f"trace={TRACED}", "-o", str(trace),
            "--", str(program), *vendor.get("config_args", []), *extra_args,
        ]
        env = environment(vendor, tree)
        executable = Path(program)
        if not executable.is_absolute():
            resolved = (
                executable.resolve()
                if os.sep in str(program)
                else Path(shutil.which(str(program), path=env["PATH"]) or str(program))
            )
            if resolved.is_absolute():
                executable = resolved
                command[command.index(str(program))] = str(executable)
        process = subprocess.Popen(
            command, cwd=run, env=env,
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            start_new_session=True,
        )
        try:
            code = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
            code = None
        lines = trace.read_text(errors="replace").splitlines() if trace.exists() else []
        events, unresolved = events_of(lines, str(run))
        started = any(
            (found := LINE.match(line.strip())) is not None
            and found.group("call") in ("execve", "execveat")
            and found.group("ret") == "0"
            and split_args(found.group("args"))[0].strip().startswith('"')
            and unquote(split_args(found.group("args"))[0].strip()[1:-1])
            == str(executable)
            for line in lines
        )
        if not started:
            code = None
        return outside(events, tree), unresolved, code


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--vendor", required=True)
    parser.add_argument("--program", required=True)
    parser.add_argument("--catalog", default=str(CATALOG))
    parser.add_argument("--allowed", default=str(ALLOWED))
    parser.add_argument("--timeout", type=int, default=60)
    parser.add_argument("args", nargs="*", help="arguments of the tool, after --")
    options = parser.parse_args()

    catalog = json.loads(Path(options.catalog).read_text())
    vendor = next((v for v in catalog["vendors"] if v["vendor"] == options.vendor), None)
    if vendor is None:
        sys.exit(f"check-containment: the catalog has no vendor {options.vendor}")
    allowed = json.loads(Path(options.allowed).read_text())

    found, unresolved, code = measure(vendor, options.program, options.args, options.timeout)
    for op, argument in unresolved:
        print(f"unresolved {op}: {argument}")
    for op, path in sorted(set(found)):
        print(f"outside the tree: {op} {path}")
    bad = offences(found, allowed_patterns(allowed, options.vendor))
    print(f"exit code of the tool: {code if code is not None else 'timeout (stopped)'}")
    # Claude's unauthenticated `-p ping` probe exits 1 after successfully starting the CLI.
    # Other exits indicate that the probe did not reach its expected terminal state.
    expected_exit_codes = {"claude": {0, 1}, "codex": {0}}
    if code not in expected_exit_codes[options.vendor]:
        print("containment measurement failed or timed out; the version must not be added")
        return 1
    if bad:
        print(f"{len(bad)} write(s) outside the vault that are not listed in allowed-residuals.json")
        for op, path in bad:
            print(f"  NOT LISTED {op} {path}")
        return 1
    print("only listed residuals (or none) outside the vault")
    return 0


if __name__ == "__main__":
    sys.exit(main())
