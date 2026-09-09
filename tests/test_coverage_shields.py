"""Unit tests for update_coverage_shields.py."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

# Add scripts directory to path for import
sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts"))

from update_coverage_shields import get_shield_color, update_or_add_badge


class TestCoverageShields(unittest.TestCase):
    def test_get_shield_color(self):
        self.assertEqual(get_shield_color(100.0), "brightgreen")
        self.assertEqual(get_shield_color(95.0), "brightgreen")
        self.assertEqual(get_shield_color(90.0), "green")
        self.assertEqual(get_shield_color(85.0), "yellowgreen")
        self.assertEqual(get_shield_color(75.0), "yellow")
        self.assertEqual(get_shield_color(65.0), "orange")
        self.assertEqual(get_shield_color(50.0), "red")

    def test_update_existing_simple_badges(self):
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

    def test_update_existing_linked_badges(self):
        content = "[![Doc Coverage](https://img.shields.io/badge/Doc%20Coverage-50.0%25-red)](https://example.com)\n"
        updated, changed = update_or_add_badge(content, "Doc Coverage", 98.5)
        self.assertTrue(changed)
        self.assertIn("Doc%20Coverage-98.5%25-brightgreen", updated)
        self.assertIn("(https://example.com)", updated)

    def test_add_doc_badge_when_test_badge_present(self):
        content = (
            "# Title\n\n"
            "![Test Coverage](https://img.shields.io/badge/Test%20Coverage-100.0%25-brightgreen)\n"
        )
        updated, changed = update_or_add_badge(content, "Doc Coverage", 100.0)
        self.assertTrue(changed)
        self.assertIn("![Test Coverage]", updated)
        self.assertIn("![Doc Coverage]", updated)
        # Doc Coverage should be right after Test Coverage
        test_pos = updated.index("![Test Coverage]")
        doc_pos = updated.index("![Doc Coverage]")
        self.assertGreater(doc_pos, test_pos)

    def test_add_test_badge_when_doc_badge_present(self):
        content = (
            "# Title\n\n"
            "![Doc Coverage](https://img.shields.io/badge/Doc%20Coverage-100.0%25-brightgreen)\n"
        )
        updated, changed = update_or_add_badge(content, "Test Coverage", 100.0)
        self.assertTrue(changed)
        self.assertIn("![Test Coverage]", updated)
        self.assertIn("![Doc Coverage]", updated)
        # Test Coverage should be before Doc Coverage
        test_pos = updated.index("![Test Coverage]")
        doc_pos = updated.index("![Doc Coverage]")
        self.assertLess(test_pos, doc_pos)

    def test_add_badges_when_only_license_badge_present(self):
        content = (
            "# Title\n\n"
            "[![License](https://img.shields.io/badge/license-MIT-blue.svg)](https://example.com)\n\n"
            "Some description.\n"
        )
        updated, changed = update_or_add_badge(content, "Doc Coverage", 100.0)
        self.assertTrue(changed)
        self.assertIn("![Doc Coverage]", updated)
        self.assertIn("Some description.", updated)

    def test_add_badges_when_no_badges_present(self):
        content = "# Project Title\n\nA cool project description.\n"
        updated, changed = update_or_add_badge(content, "Doc Coverage", 100.0)
        self.assertTrue(changed)
        self.assertIn("![Doc Coverage]", updated)
        self.assertTrue(updated.startswith("# Project Title\n\n![Doc Coverage]"))


if __name__ == "__main__":
    unittest.main()
