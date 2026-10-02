#!/usr/bin/env python3
"""
Updates or adds '% doc coverage' and '% test coverage' shields in README.md.

Calculates actual workspace doc coverage using rustdoc --show-coverage
and workspace test coverage using cargo-llvm-cov (or cargo-tarpaulin fallback).
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path


def get_shield_color(percent: float) -> str:
    """Returns the badge color corresponding to a coverage percentage.

    Args:
        percent: Coverage percentage value.

    Returns:
        The badge color corresponding to the coverage percentage.
    """
    if percent >= 95.0:
        return "brightgreen"
    if percent >= 90.0:
        return "green"
    if percent >= 80.0:
        return "yellowgreen"
    if percent >= 70.0:
        return "yellow"
    if percent >= 60.0:
        return "orange"
    return "red"


def compute_doc_coverage(repo_root: Path) -> float:
    """Computes documentation coverage percentage across all workspace crates.

    Uses rustdoc unstable --show-coverage flag to generate JSON coverage reports.

    Args:
        repo_root: Root path of the repository.

    Returns:
        The calculated doc coverage percentage.

    Raises:
        RuntimeError: If cargo doc execution fails or no doc coverage output is found.
    """
    env = dict(os.environ)
    env["RUSTDOCFLAGS"] = "-Z unstable-options --show-coverage --output-format json"
    cmd = ["cargo", "doc", "--workspace", "--no-deps"]

    result = subprocess.run(
        cmd,
        cwd=repo_root,
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        # Fallback to text format if json is unsupported
        env["RUSTDOCFLAGS"] = "-Z unstable-options --show-coverage"
        result = subprocess.run(
            cmd,
            cwd=repo_root,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode != 0:
            raise RuntimeError(f"cargo doc failed: {result.stderr}")

    # Inspect target/doc/*.json
    json_files = list(repo_root.glob("target/doc/*.json"))
    if json_files:
        total_items = 0
        documented_items = 0
        for p in json_files:
            try:
                with open(p, "r", encoding="utf-8") as f:
                    data = json.load(f)
                    for stats in data.values():
                        total_items += stats.get("total", 0)
                        documented_items += stats.get("with_docs", 0)
            except (OSError, json.JSONDecodeError, KeyError, TypeError, ValueError) as e:
                print(f"Warning: could not read {p}: {e}", file=sys.stderr)

        if total_items > 0:
            return float(round((documented_items / total_items) * 100.0, 1))

    # Inspect target/doc/*.txt if no JSON files
    txt_files = list(repo_root.glob("target/doc/*.txt"))
    if txt_files:
        total_items = 0
        documented_items = 0
        total_re = re.compile(r"\|\s*Total\s*\|\s*(\d+)\s*\|\s*([\d.]+)%\s*\|")
        for p in txt_files:
            try:
                with open(p, "r", encoding="utf-8") as f:
                    for line in f:
                        m = total_re.search(line)
                        if m:
                            doc_count = int(m.group(1))
                            pct = float(m.group(2))
                            total_est = (
                                round(doc_count / (pct / 100.0))
                                if pct > 0
                                else doc_count
                            )
                            documented_items += doc_count
                            total_items += total_est
            except (OSError, UnicodeDecodeError, ValueError) as e:
                print(f"Warning: could not read {p}: {e}", file=sys.stderr)

        if total_items > 0:
            return float(round((documented_items / total_items) * 100.0, 1))

    raise RuntimeError("No doc coverage output found in target/doc/")


def compute_test_coverage(repo_root: Path) -> float:
    """Computes test coverage percentage across all workspace crates.

    Prefers cargo-llvm-cov, with fallback to cargo-tarpaulin.

    Args:
        repo_root: Root path of the repository.

    Returns:
        The calculated test coverage percentage.

    Raises:
        RuntimeError: If neither cargo-llvm-cov nor cargo-tarpaulin succeeds in computing test coverage.
    """
    if shutil.which("cargo-llvm-cov"):
        cmd = [
            "cargo",
            "llvm-cov",
            "--all-features",
            "--workspace",
            "--branch",
            "--include-build-script",
            "--json",
            "--summary-only",
        ]
        result = subprocess.run(
            cmd,
            cwd=repo_root,
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode == 0 and result.stdout.strip():
            try:
                data = json.loads(result.stdout)
                totals = data["data"][0]["totals"]
                lines = totals["lines"]
                count = lines.get("count", 0)
                covered = lines.get("covered", 0)
                if count > 0:
                    return float(round((covered / count) * 100.0, 1))
                percent = lines.get("percent", 0.0)
                return float(round(float(percent), 1))
            except (
                json.JSONDecodeError,
                KeyError,
                IndexError,
                TypeError,
                ValueError,
            ) as e:
                print(f"Warning: failed to parse llvm-cov json: {e}", file=sys.stderr)

    if shutil.which("cargo-tarpaulin"):
        out_dir = repo_root / "target" / "tarpaulin"
        out_dir.mkdir(parents=True, exist_ok=True)
        cmd = [
            "cargo",
            "tarpaulin",
            "--workspace",
            "--timeout",
            "120",
            "--out",
            "Json",
            "--output-dir",
            str(out_dir),
        ]
        result = subprocess.run(
            cmd,
            cwd=repo_root,
            capture_output=True,
            text=True,
            check=False,
        )
        tarpaulin_json = out_dir / "tarpaulin-report.json"
        if tarpaulin_json.exists():
            try:
                with open(tarpaulin_json, "r", encoding="utf-8") as f:
                    data = json.load(f)
                    files = data.get("files", {})
                    covered = sum(len(f.get("covered", [])) for f in files.values())
                    coverable = sum(f.get("coverable", 0) for f in files.values())
                    if coverable > 0:
                        return float(round((covered / coverable) * 100.0, 1))
            except (OSError, json.JSONDecodeError, KeyError, TypeError, ValueError) as e:
                print(f"Warning: failed to read tarpaulin report: {e}", file=sys.stderr)

        match = re.search(r"([\d.]+)%\s+coverage", result.stdout)
        if match:
            return float(round(float(match.group(1)), 1))

    raise RuntimeError(
        "Neither cargo-llvm-cov nor cargo-tarpaulin succeeded in computing test coverage."
    )


def update_or_add_badge(
    readme_text: str, badge_type: str, percent: float
) -> tuple[str, bool]:
    """Updates or adds a shield for badge_type ('Doc Coverage' or 'Test Coverage') in readme_text.

    Args:
        readme_text: Original markdown content of README.
        badge_type: Name of the badge ('Doc Coverage' or 'Test Coverage').
        percent: Percentage value to set on the badge.

    Returns:
        Tuple of (new_readme_text, was_modified).
    """
    color = get_shield_color(percent)
    escaped_type = badge_type.replace(" ", "%20")
    new_badge_url = (
        f"https://img.shields.io/badge/{escaped_type}-{percent:.1f}%25-{color}"
    )
    new_badge = f"![{badge_type}]({new_badge_url})"

    # Check if badge exists with link: [![Badge](badge_url)](link_url)
    link_pattern = re.compile(
        rf"(\[!\[{re.escape(badge_type)}\]\()https://img\.shields\.io/badge/{re.escape(escaped_type)}-[^)]+(\)\]\([^)]+\))",
        re.IGNORECASE,
    )
    if link_pattern.search(readme_text):
        new_text = link_pattern.sub(rf"\g<1>{new_badge_url}\g<2>", readme_text)
        return new_text, (new_text != readme_text)

    # Check if simple badge exists: ![Badge](badge_url)
    simple_pattern = re.compile(
        rf"!\[{re.escape(badge_type)}\]\(https://img\.shields\.io/badge/{re.escape(escaped_type)}-[^)]+\)",
        re.IGNORECASE,
    )
    if simple_pattern.search(readme_text):
        new_text = simple_pattern.sub(new_badge, readme_text)
        return new_text, (new_text != readme_text)

    # If badge does not exist, add it relative to other coverage badge
    other_type = "Test Coverage" if badge_type == "Doc Coverage" else "Doc Coverage"
    other_simple = re.compile(
        rf"(!\[{re.escape(other_type)}\]\([^\)]+\))",
        re.IGNORECASE,
    )
    if other_simple.search(readme_text):
        if badge_type == "Doc Coverage":
            new_text = other_simple.sub(rf"\g<1>\n{new_badge}", readme_text, count=1)
        else:
            new_text = other_simple.sub(rf"{new_badge}\n\g<1>", readme_text, count=1)
        return new_text, True

    # If no other coverage badge, insert after the last existing badge
    any_badge_pattern = re.compile(
        r"(\[!\[[^\]]+\]\([^\)]+\)\]\([^\)]+\)|!\[[^\]]+\]\([^\)]+\))"
    )
    badges = list(any_badge_pattern.finditer(readme_text))
    if badges:
        last_badge = badges[-1]
        insert_pos = last_badge.end()
        new_text = (
            readme_text[:insert_pos] + "\n" + new_badge + readme_text[insert_pos:]
        )
        return new_text, True

    # Fallback: after first heading
    lines = readme_text.splitlines(keepends=True)
    insert_idx = 0
    for idx, line in enumerate(lines):
        if line.startswith(("#", "===", "---")):
            insert_idx = idx + 1
            while insert_idx < len(lines) and lines[insert_idx].strip() == "":
                insert_idx += 1
            break

    lines.insert(insert_idx, new_badge + "\n\n")
    return "".join(lines), True


def main(argv: list[str] | None = None) -> int:
    """Entry point to update or add coverage shields in README.md.

    Args:
        argv: Optional list of CLI arguments (defaults to sys.argv[1:] if None).

    Returns:
        int: 0 on success without modifications, 1 if file was modified or on error.
    """
    parser = argparse.ArgumentParser(
        description="Update or add coverage shields in README.md."
    )
    parser.add_argument(
        "--doc",
        action="store_true",
        help="Update '%% doc coverage' shield in README.md",
    )
    parser.add_argument(
        "--test",
        action="store_true",
        help="Update '%% test coverage' shield in README.md",
    )
    parser.add_argument(
        "--readme",
        type=Path,
        default=Path("README.md"),
        help="Path to README.md",
    )
    args = parser.parse_args(argv)

    do_doc = args.doc or not (args.doc or args.test)
    do_test = args.test or not (args.doc or args.test)

    repo_root = Path(__file__).resolve().parent.parent
    readme_path = (
        args.readme if args.readme.is_absolute() else (repo_root / args.readme)
    )

    if not readme_path.exists():
        print(f"Error: {readme_path} does not exist.", file=sys.stderr)
        return 1

    with open(readme_path, "r", encoding="utf-8") as f:
        readme_content = f.read()

    modified = False

    if do_doc:
        try:
            doc_pct = compute_doc_coverage(repo_root)
            print(f"Computed Doc Coverage: {doc_pct:.1f}%")
            readme_content, changed = update_or_add_badge(
                readme_content, "Doc Coverage", doc_pct
            )
            if changed:
                modified = True
                print(f"Updated '% doc coverage' shield to {doc_pct:.1f}%")
            else:
                print(f"'% doc coverage' shield is already up to date ({doc_pct:.1f}%)")
        except (RuntimeError, OSError) as e:
            print(f"Error computing doc coverage: {e}", file=sys.stderr)
            return 1

    if do_test:
        try:
            test_pct = compute_test_coverage(repo_root)
            print(f"Computed Test Coverage: {test_pct:.1f}%")
            readme_content, changed = update_or_add_badge(
                readme_content, "Test Coverage", test_pct
            )
            if changed:
                modified = True
                print(f"Updated '% test coverage' shield to {test_pct:.1f}%")
            else:
                print(
                    f"'% test coverage' shield is already up to date ({test_pct:.1f}%)"
                )
        except (RuntimeError, OSError) as e:
            print(f"Error computing test coverage: {e}", file=sys.stderr)
            return 1

    if modified:
        with open(readme_path, "w", encoding="utf-8") as f:
            f.write(readme_content)
        print(f"Successfully saved changes to {readme_path.name}")
        return 0

    return 0


if __name__ == "__main__":
    sys.exit(main())
