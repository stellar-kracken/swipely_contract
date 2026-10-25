#!/usr/bin/env python3
"""Fail CI when a compiled Soroban contract exceeds its committed WASM budget."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_BUDGETS = ROOT / "scripts" / "wasm-size-budgets.json"
DEFAULT_ARTIFACTS = ROOT / "target" / "wasm32-unknown-unknown" / "release"


def check_sizes(artifacts_dir: Path, budgets_file: Path, summary_file: Path | None = None) -> int:
    """Print actual/baseline/budget bytes and return 0 only for a complete passing set."""
    try:
        config = json.loads(budgets_file.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        print(f"ERROR: cannot read WASM budget config {budgets_file}: {exc}", file=sys.stderr)
        return 2

    entries = config.get("artifacts") if isinstance(config, dict) else None
    if not isinstance(config, dict) or config.get("schema_version") != 1 or not isinstance(entries, dict) or not entries:
        print("ERROR: expected a nonempty artifacts map and schema_version=1", file=sys.stderr)
        return 2

    for name, values in entries.items():
        if (
            not isinstance(name, str)
            or Path(name).name != name
            or not name.endswith(".wasm")
            or not isinstance(values, dict)
            or type(values.get("baseline_bytes")) is not int
            or type(values.get("budget_bytes")) is not int
            or values["baseline_bytes"] <= 0
            or values["budget_bytes"] < values["baseline_bytes"]
        ):
            print(f"ERROR: invalid baseline/budget for {name!r}", file=sys.stderr)
            return 2

    files = {file.name: file for file in artifacts_dir.glob("*.wasm") if file.is_file()}
    missing = sorted(set(entries) - set(files))
    unbudgeted = sorted(set(files) - set(entries))
    results: list[tuple[str, int, int, int]] = []
    exceeded = []
    for name in sorted(set(entries) & set(files)):
        size = files[name].stat().st_size
        baseline, limit = entries[name]["baseline_bytes"], entries[name]["budget_bytes"]
        results.append((name, size, baseline, limit))
        if size > limit:
            exceeded.append(name)

    print("Soroban release WASM artifact size budget (bytes; 1 KiB = 1024 bytes)")
    print(f"{'Artifact':32} {'Actual':>24} {'Baseline (B)':>16} {'Limit (B)':>16} {'Delta (B)':>12}  Status")
    lines = ["## Soroban release WASM sizes", "", "| Artifact | Actual | Baseline | Budget | Delta | Status |", "| --- | ---: | ---: | ---: | ---: | --- |"]
    for name, size, baseline, limit in results:
        delta = size - baseline
        status = "OVER BUDGET" if size > limit else "OK"
        actual_display = f"{size:,} B ({size / 1024:.1f} KiB)"
        print(f"{name:32} {actual_display:>24} {baseline:16,} {limit:16,} {delta:+12,}  {status}")
        lines.append(f"| `{name}` | {size:,} B | {baseline:,} B | {limit:,} B | {delta:+,} B | {status} |")

    errors = []
    if missing:
        errors.append("missing release artifacts: " + ", ".join(missing))
    if unbudgeted:
        errors.append("unbudgeted release artifacts: " + ", ".join(unbudgeted))
    if exceeded:
        errors.append("size limit exceeded: " + ", ".join(exceeded))
    if not files:
        errors.append(f"no *.wasm artifacts found in {artifacts_dir}; did cargo build --release --target wasm32-unknown-unknown run?")

    if errors:
        for message in errors:
            print("ERROR: " + message, file=sys.stderr)
            lines.extend(["", "**FAIL:** " + message])
    else:
        print("PASS: all expected release WASM artifacts are within budget")
        lines.append("\n**PASS:** All release WASM binaries are within budget.")

    if summary_file is not None:
        try:
            with summary_file.open("a", encoding="utf-8") as summary:
                summary.write("\n".join(lines) + "\n")
        except OSError as exc:
            print(f"WARNING: cannot write GitHub job summary: {exc}", file=sys.stderr)

    return 1 if errors else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifacts-dir", type=Path, default=DEFAULT_ARTIFACTS)
    parser.add_argument("--budgets", type=Path, default=DEFAULT_BUDGETS)
    args = parser.parse_args()
    summary = Path(os.environ["GITHUB_STEP_SUMMARY"]) if os.environ.get("GITHUB_STEP_SUMMARY") else None
    return check_sizes(args.artifacts_dir, args.budgets, summary)


if __name__ == "__main__":
    raise SystemExit(main())
