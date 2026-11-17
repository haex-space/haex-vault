"""check-device-traces.py finds a needle outside vaults/ in any spelling, and nowhere inside."""

import importlib.util
import tempfile
import unittest
from pathlib import Path
from urllib.parse import quote

SPEC = importlib.util.spec_from_file_location(
    "check_device_traces", Path(__file__).with_name("check-device-traces.py")
)
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)

VAULT = "Geheimtresor 4711"


def write(root, relative, data):
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data if isinstance(data, bytes) else data.encode())
    return path


class DeviceTracesTest(unittest.TestCase):
    def test_a_clean_tree_has_no_trace_and_the_vault_folder_may_name_itself(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write(root, "logs/app.log", "started, nothing else\n")
            write(root, f"vaults/{VAULT}/home/records/tools.log", f"{VAULT} install\n")
            self.assertEqual(CHECK.traces(root, [VAULT]), [])

    def test_a_needle_is_found_as_written_in_utf16_and_percent_encoded(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write(root, "logs/app.log", f"opened {VAULT}\n")
            write(root, "storage/leveldb/000003.log", VAULT.encode("utf-16-le"))
            write(root, "localstorage/page", f"https://x/?v={quote(VAULT, safe='')}")
            write(root, "cookies", "nothing")
            found = {(name, label) for name, _, label in CHECK.traces(root, [VAULT])}
            self.assertEqual(
                found,
                {
                    ("logs/app.log", "as written"),
                    ("storage/leveldb/000003.log", "as UTF-16"),
                    ("localstorage/page", "percent-encoded"),
                },
            )

    def test_a_needle_in_a_file_name_and_across_a_chunk_edge_is_found(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write(root, f"WebKitCache/{VAULT}.json", "{}")
            edge = b"a" * (CHECK.CHUNK - 3) + VAULT.encode() + b"b"
            write(root, "logs/big.log", edge)
            found = {(name, label) for name, _, label in CHECK.traces(root, [VAULT])}
            self.assertEqual(
                found,
                {
                    (f"WebKitCache/{VAULT}.json", "in the file name"),
                    ("logs/big.log", "as written"),
                },
            )

    def test_the_command_exits_with_one_for_a_trace_and_zero_for_none(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write(root, "logs/app.log", "x\n")
            self.assertEqual(CHECK.main(["check", str(root), VAULT]), 0)
            write(root, "logs/app.log", f"{VAULT}\n")
            self.assertEqual(CHECK.main(["check", str(root), VAULT]), 1)
            self.assertEqual(CHECK.main(["check", str(root)]), 2)


if __name__ == "__main__":
    unittest.main()
