"""Runs the tests of the maintainer scripts in scripts/cli-tools (spec 094) in the same CI step as
the other script tests, which discovers only this folder."""

from pathlib import Path

CLI_TOOLS = Path(__file__).resolve().parents[1] / "cli-tools"


def load_tests(loader, tests, pattern):
    tests.addTests(loader.discover(str(CLI_TOOLS), pattern="test_*.py", top_level_dir=str(CLI_TOOLS)))
    return tests
