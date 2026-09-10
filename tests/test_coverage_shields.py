"""Unit tests for update_coverage_shields.py."""

from __future__ import annotations

import io
import json
import runpy
import sys
import unittest
from pathlib import Path
from unittest.mock import MagicMock, mock_open, patch

# Add scripts directory to path for import
sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts"))

from update_coverage_shields import (
    compute_doc_coverage,
    compute_test_coverage,
    get_shield_color,
    main,
    update_or_add_badge,
)


class TestCoverageShields(unittest.TestCase):
    """Test suite covering coverage shields calculation, parsing, and CLI entry points."""

    def test_get_shield_color(self) -> None:
        """Tests get_shield_color for all coverage percentage thresholds."""
        self.assertEqual(get_shield_color(100.0), "brightgreen")
        self.assertEqual(get_shield_color(95.0), "brightgreen")
        self.assertEqual(get_shield_color(90.0), "green")
        self.assertEqual(get_shield_color(85.0), "yellowgreen")
        self.assertEqual(get_shield_color(75.0), "yellow")
        self.assertEqual(get_shield_color(65.0), "orange")
        self.assertEqual(get_shield_color(50.0), "red")

    def test_update_existing_simple_badges(self) -> None:
        """Tests updating simple inline markdown badges and idempotency."""
        content = (
            "# Title\n\n"
            "![Test Coverage](https://img.shields.io/badge/Test%20Coverage-50.0%25-red)\n"
            "![Doc Coverage](https://img.shields.io/badge/Doc%20Coverage-80.0%25-yellowgreen)\n"
        )

        updated, changed = update_or_add_badge(content, "Test Coverage", 100.0)
        self.assertTrue(changed)
        self.assertIn("Test%20Coverage-100.0%25-brightgreen", updated)

        updated2, changed2 = update_or_add_badge(updated, "Doc Coverage", 100.0)
        self.assertTrue(changed2)
        self.assertIn("Doc%20Coverage-100.0%25-brightgreen", updated2)

        # Re-running on already updated content should not modify
        _, changed3 = update_or_add_badge(updated2, "Test Coverage", 100.0)
        self.assertFalse(changed3)
        _, changed4 = update_or_add_badge(updated2, "Doc Coverage", 100.0)
        self.assertFalse(changed4)

    def test_update_existing_linked_badges(self) -> None:
        """Tests updating badges that are wrapped in hyperlink markdown."""
        content = "[![Doc Coverage](https://img.shields.io/badge/Doc%20Coverage-50.0%25-red)](https://example.com)\n"
        updated, changed = update_or_add_badge(content, "Doc Coverage", 98.5)
        self.assertTrue(changed)
        self.assertIn("Doc%20Coverage-98.5%25-brightgreen", updated)
        self.assertIn("(https://example.com)", updated)

    def test_add_doc_badge_when_test_badge_present(self) -> None:
        """Tests inserting Doc Coverage badge directly after existing Test Coverage badge."""
        content = (
            "# Title\n\n"
            "![Test Coverage](https://img.shields.io/badge/Test%20Coverage-100.0%25-brightgreen)\n"
        )
        updated, changed = update_or_add_badge(content, "Doc Coverage", 100.0)
        self.assertTrue(changed)
        self.assertIn("![Test Coverage]", updated)
        self.assertIn("![Doc Coverage]", updated)
        test_pos = updated.index("![Test Coverage]")
        doc_pos = updated.index("![Doc Coverage]")
        self.assertGreater(doc_pos, test_pos)

    def test_add_test_badge_when_doc_badge_present(self) -> None:
        """Tests inserting Test Coverage badge directly before existing Doc Coverage badge."""
        content = (
            "# Title\n\n"
            "![Doc Coverage](https://img.shields.io/badge/Doc%20Coverage-100.0%25-brightgreen)\n"
        )
        updated, changed = update_or_add_badge(content, "Test Coverage", 100.0)
        self.assertTrue(changed)
        self.assertIn("![Test Coverage]", updated)
        self.assertIn("![Doc Coverage]", updated)
        test_pos = updated.index("![Test Coverage]")
        doc_pos = updated.index("![Doc Coverage]")
        self.assertLess(test_pos, doc_pos)

    def test_add_badges_when_only_license_badge_present(self) -> None:
        """Tests appending a badge after existing non-coverage badges."""
        content = (
            "# Title\n\n"
            "[![License](https://img.shields.io/badge/license-MIT-blue.svg)](https://example.com)\n\n"
            "Some description.\n"
        )
        updated, changed = update_or_add_badge(content, "Doc Coverage", 100.0)
        self.assertTrue(changed)
        self.assertIn("![Doc Coverage]", updated)
        self.assertIn("Some description.", updated)

    def test_add_badges_when_no_badges_present(self) -> None:
        """Tests inserting a badge after markdown headers when no existing badges exist."""
        content = "# Project Title\n\nA cool project description.\n"
        updated, changed = update_or_add_badge(content, "Doc Coverage", 100.0)
        self.assertTrue(changed)
        self.assertIn("![Doc Coverage]", updated)
        self.assertTrue(updated.startswith("# Project Title\n\n![Doc Coverage]"))

        # Underline style headers (=== and ---)
        underline_content = "Project Title\n===\n\nA description.\n"
        updated_under, changed_under = update_or_add_badge(
            underline_content, "Doc Coverage", 100.0
        )
        self.assertTrue(changed_under)
        self.assertIn("![Doc Coverage]", updated_under)

        # No header at all
        no_header = "Just plain text\n"
        updated_none, changed_none = update_or_add_badge(
            no_header, "Doc Coverage", 100.0
        )
        self.assertTrue(changed_none)
        self.assertTrue(updated_none.startswith("![Doc Coverage]"))

    @patch("subprocess.run")
    def test_compute_doc_coverage_json_success(self, mock_run: MagicMock) -> None:
        """Tests computing doc coverage from rustdoc JSON output files.

        Args:
            mock_run: Mock subprocess.run object.
        """
        mock_run.return_value = MagicMock(returncode=0, stdout="", stderr="")
        repo_root = Path("/dummy/repo")

        json_file = MagicMock()
        mock_data = json.dumps({"crate": {"total": 100, "with_docs": 95}})

        with patch.object(Path, "glob", return_value=[json_file]), patch(
            "builtins.open", mock_open(read_data=mock_data)
        ):
            pct = compute_doc_coverage(repo_root)
            self.assertEqual(pct, 95.0)

    @patch("subprocess.run")
    def test_compute_doc_coverage_json_read_error(self, mock_run: MagicMock) -> None:
        """Tests computing doc coverage handling corrupt JSON output files gracefully.

        Args:
            mock_run: Mock subprocess.run object.
        """
        mock_run.return_value = MagicMock(returncode=0, stdout="", stderr="")
        repo_root = Path("/dummy/repo")

        json_file1 = MagicMock()
        json_file2 = MagicMock()

        def fake_open(p: MagicMock, *args: object, **kwargs: object) -> io.StringIO:
            """Simulates read error on json_file1.

            Args:
                p: Target path.
                *args: Extra args.
                **kwargs: Extra kwargs.

            Returns:
                io.StringIO: Buffer.

            Raises:
                OSError: Simulating file read error.
            """
            if p == json_file1:
                raise OSError("corrupt file")
            return io.StringIO(json.dumps({"crate": {"total": 50, "with_docs": 50}}))

        with patch.object(Path, "glob", return_value=[json_file1, json_file2]), patch(
            "builtins.open", side_effect=fake_open
        ), patch("sys.stderr", new_callable=io.StringIO):
            pct = compute_doc_coverage(repo_root)
            self.assertEqual(pct, 100.0)

    @patch("subprocess.run")
    def test_compute_doc_coverage_json_zero_items_falls_through(
        self, mock_run: MagicMock
    ) -> None:
        """Tests falling through to txt report when JSON report has total == 0.

        Args:
            mock_run: Mock subprocess.run object.
        """
        mock_run.return_value = MagicMock(returncode=0, stdout="", stderr="")
        repo_root = Path("/dummy/repo")

        json_file = MagicMock()
        txt_file = MagicMock()
        mock_data = json.dumps({"crate": {"total": 0, "with_docs": 0}})
        table_output = "| Total | 90 | 90.0% |\n"

        def fake_glob(pattern: str) -> list[MagicMock]:
            """Mocks glob to separate json and txt patterns.

            Args:
                pattern: Glob search pattern.

            Returns:
                list[MagicMock]: Matching path list.
            """
            if "json" in pattern:
                return [json_file]
            return [txt_file]

        def fake_open(p: MagicMock, *args: object, **kwargs: object) -> io.StringIO:
            """Mocks open returning zero json then valid text.

            Args:
                p: Target path.
                *args: Extra args.
                **kwargs: Extra kwargs.

            Returns:
                io.StringIO: Buffer.
            """
            if p == json_file:
                return io.StringIO(mock_data)
            return io.StringIO(table_output)

        with patch.object(Path, "glob", side_effect=fake_glob), patch(
            "builtins.open", side_effect=fake_open
        ):
            pct = compute_doc_coverage(repo_root)
            self.assertEqual(pct, 90.0)

    @patch("subprocess.run")
    def test_compute_doc_coverage_txt_fallback(self, mock_run: MagicMock) -> None:
        """Tests falling back to rustdoc text table output when JSON format is unsupported.

        Args:
            mock_run: Mock subprocess.run object.
        """
        mock_run.side_effect = [
            MagicMock(returncode=1, stdout="", stderr="unsupported"),
            MagicMock(returncode=0, stdout="", stderr=""),
        ]
        repo_root = Path("/dummy/repo")
        txt_file = MagicMock()
        table_output = (
            "+------------------+------------+------------+\n"
            "| Total            |         90 |      90.0% |\n"
            "+------------------+------------+------------+\n"
        )

        with patch.object(
            Path,
            "glob",
            side_effect=lambda pattern: [] if "json" in pattern else [txt_file],
        ), patch("builtins.open", mock_open(read_data=table_output)):
            pct = compute_doc_coverage(repo_root)
            self.assertEqual(pct, 90.0)

    @patch("subprocess.run")
    def test_compute_doc_coverage_txt_zero_percent_and_read_error(
        self, mock_run: MagicMock
    ) -> None:
        """Tests text table parsing when coverage is zero percent and handling text read errors.

        Args:
            mock_run: Mock subprocess.run object.
        """
        mock_run.return_value = MagicMock(returncode=0, stdout="", stderr="")
        repo_root = Path("/dummy/repo")
        bad_txt = MagicMock()
        zero_txt = MagicMock()
        zero_output = "| Total | 0 | 0.0% |\n"

        def fake_open(p: MagicMock, *args: object, **kwargs: object) -> io.StringIO:
            """Simulates decoding error on bad_txt and zero output on zero_txt.

            Args:
                p: Target path.
                *args: Extra args.
                **kwargs: Extra kwargs.

            Returns:
                io.StringIO: Buffer.

            Raises:
                UnicodeDecodeError: Simulating invalid decode.
            """
            if p == bad_txt:
                raise UnicodeDecodeError("utf-8", b"", 0, 1, "invalid")
            return io.StringIO(zero_output)

        with patch.object(
            Path,
            "glob",
            side_effect=lambda pattern: []
            if "json" in pattern
            else [bad_txt, zero_txt],
        ), patch("builtins.open", side_effect=fake_open), patch(
            "sys.stderr", new_callable=io.StringIO
        ), self.assertRaises(RuntimeError):
            compute_doc_coverage(repo_root)

    @patch("subprocess.run")
    def test_compute_doc_coverage_cargo_doc_failure(self, mock_run: MagicMock) -> None:
        """Tests RuntimeError raised when cargo doc process execution fails.

        Args:
            mock_run: Mock subprocess.run object.
        """
        mock_run.side_effect = [
            MagicMock(returncode=1, stdout="", stderr="fail 1"),
            MagicMock(returncode=1, stdout="", stderr="fail 2"),
        ]
        with self.assertRaises(RuntimeError):
            compute_doc_coverage(Path("/dummy/repo"))

    @patch("subprocess.run")
    def test_compute_doc_coverage_no_output_files(self, mock_run: MagicMock) -> None:
        """Tests RuntimeError raised when no doc reports are found in target directory.

        Args:
            mock_run: Mock subprocess.run object.
        """
        mock_run.return_value = MagicMock(returncode=0, stdout="", stderr="")
        with patch.object(Path, "glob", return_value=[]), self.assertRaises(RuntimeError):
            compute_doc_coverage(Path("/dummy/repo"))

    @patch("shutil.which")
    @patch("subprocess.run")
    def test_compute_test_coverage_llvm_cov_success(
        self, mock_run: MagicMock, mock_which: MagicMock
    ) -> None:
        """Tests cargo-llvm-cov computation with line counts and fallback percent.

        Args:
            mock_run: Mock subprocess.run object.
            mock_which: Mock shutil.which object.
        """
        mock_which.side_effect = lambda cmd: "/usr/bin/" + cmd
        repo_root = Path("/dummy/repo")

        # 1. With lines count > 0
        llvm_json = json.dumps(
            {"data": [{"totals": {"lines": {"count": 200, "covered": 180}}}]}
        )
        mock_run.return_value = MagicMock(returncode=0, stdout=llvm_json, stderr="")
        pct = compute_test_coverage(repo_root)
        self.assertEqual(pct, 90.0)

        # 2. With count == 0 and percent key
        llvm_json_percent = json.dumps(
            {"data": [{"totals": {"lines": {"count": 0, "percent": 88.4}}}]}
        )
        mock_run.return_value = MagicMock(
            returncode=0, stdout=llvm_json_percent, stderr=""
        )
        pct2 = compute_test_coverage(repo_root)
        self.assertEqual(pct2, 88.4)

    @patch("shutil.which")
    @patch("subprocess.run")
    def test_compute_test_coverage_llvm_cov_failure_falls_through(
        self, mock_run: MagicMock, mock_which: MagicMock
    ) -> None:
        """Tests falling back to tarpaulin when llvm-cov returns non-zero code.

        Args:
            mock_run: Mock subprocess.run object.
            mock_which: Mock shutil.which object.
        """
        mock_which.side_effect = lambda cmd: "/usr/bin/" + cmd
        repo_root = Path("/dummy/repo")

        # llvm-cov fails with returncode 1, tarpaulin succeeds with regex
        mock_run.side_effect = [
            MagicMock(returncode=1, stdout="", stderr="error"),
            MagicMock(returncode=0, stdout="85.0% coverage", stderr=""),
        ]

        with patch.object(Path, "mkdir"), patch.object(Path, "exists", return_value=False):
            pct = compute_test_coverage(repo_root)
            self.assertEqual(pct, 85.0)

    @patch("shutil.which")
    @patch("subprocess.run")
    def test_compute_test_coverage_tarpaulin_fallback(
        self, mock_run: MagicMock, mock_which: MagicMock
    ) -> None:
        """Tests falling back to cargo-tarpaulin report file or regex when llvm-cov fails.

        Args:
            mock_run: Mock subprocess.run object.
            mock_which: Mock shutil.which object.
        """
        mock_which.side_effect = lambda cmd: "/usr/bin/" + cmd
        repo_root = Path("/dummy/repo")

        # llvm-cov outputs malformed JSON, tarpaulin outputs json report
        mock_run.side_effect = [
            MagicMock(returncode=0, stdout="not-json", stderr=""),
            MagicMock(returncode=0, stdout="", stderr=""),
        ]
        tarpaulin_data = json.dumps(
            {
                "files": {
                    "a.rs": {"covered": [1, 2], "coverable": 2},
                    "b.rs": {"covered": [1], "coverable": 2},
                }
            }
        )

        with patch.object(Path, "mkdir"), patch.object(
            Path, "exists", return_value=True
        ), patch("builtins.open", mock_open(read_data=tarpaulin_data)):
            pct = compute_test_coverage(repo_root)
            self.assertEqual(pct, 75.0)

    @patch("shutil.which")
    @patch("subprocess.run")
    def test_compute_test_coverage_tarpaulin_zero_coverable_falls_through_to_regex(
        self, mock_run: MagicMock, mock_which: MagicMock
    ) -> None:
        """Tests tarpaulin report with zero coverable lines falling back to stdout regex.

        Args:
            mock_run: Mock subprocess.run object.
            mock_which: Mock shutil.which object.
        """
        mock_which.side_effect = lambda cmd: "/usr/bin/" + cmd
        repo_root = Path("/dummy/repo")

        mock_run.side_effect = [
            MagicMock(returncode=0, stdout="not-json", stderr=""),
            MagicMock(returncode=0, stdout="Overall: 91.0% coverage", stderr=""),
        ]
        tarpaulin_data = json.dumps({"files": {}})

        with patch.object(Path, "mkdir"), patch.object(
            Path, "exists", return_value=True
        ), patch("builtins.open", mock_open(read_data=tarpaulin_data)):
            pct = compute_test_coverage(repo_root)
            self.assertEqual(pct, 91.0)

    @patch("shutil.which")
    @patch("subprocess.run")
    def test_compute_test_coverage_tarpaulin_stdout_regex_miss_raises(
        self, mock_run: MagicMock, mock_which: MagicMock
    ) -> None:
        """Tests RuntimeError when tarpaulin JSON file is missing and stdout has no regex match.

        Args:
            mock_run: Mock subprocess.run object.
            mock_which: Mock shutil.which object.
        """
        mock_which.side_effect = (
            lambda cmd: "/usr/bin/" + cmd if cmd == "cargo-tarpaulin" else None
        )
        repo_root = Path("/dummy/repo")

        mock_run.return_value = MagicMock(returncode=0, stdout="No match here", stderr="")

        with patch.object(Path, "mkdir"), patch.object(
            Path, "exists", return_value=False
        ), self.assertRaises(RuntimeError):
            compute_test_coverage(repo_root)

    @patch("shutil.which")
    @patch("subprocess.run")
    def test_compute_test_coverage_tarpaulin_stdout_regex(
        self, mock_run: MagicMock, mock_which: MagicMock
    ) -> None:
        """Tests parsing cargo-tarpaulin stdout text output when JSON report reading fails.

        Args:
            mock_run: Mock subprocess.run object.
            mock_which: Mock shutil.which object.
        """
        mock_which.side_effect = (
            lambda cmd: "/usr/bin/cargo-tarpaulin" if cmd == "cargo-tarpaulin" else None
        )
        repo_root = Path("/dummy/repo")

        mock_run.return_value = MagicMock(
            returncode=0, stdout="Coverage: 82.5% coverage", stderr=""
        )

        with patch.object(Path, "mkdir"), patch.object(
            Path, "exists", return_value=True
        ), patch("builtins.open", side_effect=OSError("read failure")), patch(
            "sys.stderr", new_callable=io.StringIO
        ):
            pct = compute_test_coverage(repo_root)
            self.assertEqual(pct, 82.5)

    @patch("shutil.which")
    def test_compute_test_coverage_neither_available(
        self, mock_which: MagicMock
    ) -> None:
        """Tests RuntimeError raised when neither coverage tool is available or successful.

        Args:
            mock_which: Mock shutil.which object.
        """
        mock_which.return_value = None
        with self.assertRaises(RuntimeError):
            compute_test_coverage(Path("/dummy/repo"))

    def test_main_help_escapes_percentage(self) -> None:
        """Tests that --help flag runs cleanly and does not raise TypeError/ValueError."""
        with self.assertRaises(SystemExit) as cm, patch(
            "sys.stdout", new_callable=io.StringIO
        ) as mock_out:
            main(["--help"])
        self.assertEqual(cm.exception.code, 0)
        self.assertIn("Update '% doc coverage' shield", mock_out.getvalue())
        self.assertIn("Update '% test coverage' shield", mock_out.getvalue())

    def test_main_nonexistent_readme(self) -> None:
        """Tests main returns 1 when specified README file does not exist."""
        with patch("sys.stderr", new_callable=io.StringIO):
            code = main(["--readme", "nonexistent_readme_12345.md"])
            self.assertEqual(code, 1)

    @patch("update_coverage_shields.compute_doc_coverage")
    @patch("update_coverage_shields.compute_test_coverage")
    def test_main_updates_both_success_and_idempotent(
        self, mock_test: MagicMock, mock_doc: MagicMock
    ) -> None:
        """Tests main execution updating shields and returning proper exit codes.

        Args:
            mock_test: Mock compute_test_coverage.
            mock_doc: Mock compute_doc_coverage.
        """
        mock_doc.return_value = 100.0
        mock_test.return_value = 100.0

        initial_readme = (
            "# Title\n\n"
            "![Test Coverage](https://img.shields.io/badge/Test%20Coverage-50.0%25-red)\n"
            "![Doc Coverage](https://img.shields.io/badge/Doc%20Coverage-50.0%25-red)\n"
        )
        updated_readme = (
            "# Title\n\n"
            "![Test Coverage](https://img.shields.io/badge/Test%20Coverage-100.0%25-brightgreen)\n"
            "![Doc Coverage](https://img.shields.io/badge/Doc%20Coverage-100.0%25-brightgreen)\n"
        )

        with patch.object(Path, "exists", return_value=True):
            # First run: modifies file -> returns 1
            m = mock_open(read_data=initial_readme)
            with patch("builtins.open", m):
                code = main([])
                self.assertEqual(code, 1)
                m().write.assert_called_once_with(updated_readme)

            # Second run on already-updated content: unmodified -> returns 0
            m2 = mock_open(read_data=updated_readme)
            with patch("builtins.open", m2):
                code2 = main([])
                self.assertEqual(code2, 0)
                m2().write.assert_not_called()

    @patch("update_coverage_shields.compute_doc_coverage")
    def test_main_doc_error_returns_one(self, mock_doc: MagicMock) -> None:
        """Tests main returns 1 when compute_doc_coverage raises RuntimeError.

        Args:
            mock_doc: Mock compute_doc_coverage.
        """
        mock_doc.side_effect = RuntimeError("doc calculation error")
        with patch.object(Path, "exists", return_value=True), patch(
            "builtins.open", mock_open(read_data="# Title\n")
        ), patch("sys.stderr", new_callable=io.StringIO):
            code = main(["--doc"])
            self.assertEqual(code, 1)

    @patch("update_coverage_shields.compute_test_coverage")
    def test_main_test_error_returns_one(self, mock_test: MagicMock) -> None:
        """Tests main returns 1 when compute_test_coverage raises RuntimeError.

        Args:
            mock_test: Mock compute_test_coverage.
        """
        mock_test.side_effect = RuntimeError("test calculation error")
        with patch.object(Path, "exists", return_value=True), patch(
            "builtins.open", mock_open(read_data="# Title\n")
        ), patch("sys.stderr", new_callable=io.StringIO):
            code = main(["--test"])
            self.assertEqual(code, 1)

    @patch("update_coverage_shields.compute_doc_coverage")
    def test_main_doc_only_flag(self, mock_doc: MagicMock) -> None:
        """Tests running main with only --doc flag without updating test badge.

        Args:
            mock_doc: Mock compute_doc_coverage.
        """
        mock_doc.return_value = 100.0
        with patch.object(Path, "exists", return_value=True), patch(
            "builtins.open",
            mock_open(
                read_data="![Doc Coverage](https://img.shields.io/badge/Doc%20Coverage-100.0%25-brightgreen)\n"
            ),
        ):
            code = main(["--doc"])
            self.assertEqual(code, 0)

    def test_main_invoked_as_module_via_runpy(self) -> None:
        """Tests invoking scripts.update_coverage_shields as __main__ using runpy."""
        with self.assertRaises(SystemExit) as cm, patch(
            "sys.argv", ["update_coverage_shields.py", "--help"]
        ), patch("sys.stdout", new_callable=io.StringIO):
            runpy.run_module("update_coverage_shields", run_name="__main__")
        self.assertEqual(cm.exception.code, 0)


if __name__ == "__main__":
    unittest.main()
