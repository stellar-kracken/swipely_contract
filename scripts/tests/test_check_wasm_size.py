"""Regression tests for the CI WASM size budget gate (no Rust build required)."""

import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from check_wasm_size import check_sizes


class CheckWasmSizeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.bin = self.root / "release"
        self.bin.mkdir()
        self.budgets = self.root / "budgets.json"
        self.save({"a.wasm": {"baseline_bytes": 10, "budget_bytes": 12}})
        (self.bin / "a.wasm").write_bytes(b"0" * 10)

    def save(self, entries):
        self.budgets.write_text(json.dumps({"schema_version": 1, "artifacts": entries}))

    def test_at_baseline_passes(self):
        self.assertEqual(check_sizes(self.bin, self.budgets), 0)

    def test_exactly_at_limit_passes(self):
        (self.bin / "a.wasm").write_bytes(b"x" * 12)
        self.assertEqual(check_sizes(self.bin, self.budgets), 0)

    def test_one_byte_over_limit_fails(self):
        (self.bin / "a.wasm").write_bytes(b"x" * 13)
        self.assertEqual(check_sizes(self.bin, self.budgets), 1)

    def test_missing_expected_binary_fails(self):
        (self.bin / "a.wasm").unlink()
        self.assertEqual(check_sizes(self.bin, self.budgets), 1)

    def test_unbudgeted_new_binary_fails(self):
        (self.bin / "new_contract.wasm").write_bytes(b"x")
        self.assertEqual(check_sizes(self.bin, self.budgets), 1)

    def test_rejects_invalid_or_negative_limits(self):
        for values in ({"baseline_bytes": 10, "budget_bytes": 9}, {"baseline_bytes": 0, "budget_bytes": 10}, {"baseline_bytes": 10, "budget_bytes": "12"}):
            with self.subTest(values=values):
                self.save({"a.wasm": values})
                self.assertEqual(check_sizes(self.bin, self.budgets), 2)

    def test_invalid_config_root_fails_cleanly(self):
        self.budgets.write_text("[]")
        self.assertEqual(check_sizes(self.bin, self.budgets), 2)

    def test_invalid_config_json_fails_cleanly(self):
        self.budgets.write_text("not JSON")
        self.assertEqual(check_sizes(self.bin, self.budgets), 2)

    def test_github_step_summary_contains_table(self):
        summary = self.root / "step_summary.md"
        self.assertEqual(check_sizes(self.bin, self.budgets, summary), 0)
        contents = summary.read_text()
        self.assertIn("a.wasm", contents)
        self.assertIn("PASS", contents)
        self.assertIn("Baseline", contents)


if __name__ == "__main__":
    unittest.main()
