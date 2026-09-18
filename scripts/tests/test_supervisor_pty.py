"""Regression for the synthetic terminal observer, not a live TUI launch."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("supervisor_pty", Path(__file__).with_name("supervisor-pty.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class TerminalDecoderTests(unittest.TestCase):
    def test_wide_characters_do_not_shift_following_cursor_addressed_text(self):
        screen = module.Utf8Screen(3, 60)
        screen.feed("界 CONFIRM RESUME".encode("utf-8"))
        screen.feed(b"\x1b[1;12HPAUSE ")
        self.assertIn("CONFIRM PAUSE", str(screen))

    def test_multibyte_border_and_partial_reads_preserve_cursor_addressed_confirmation(self):
        initial = "│CONFIRM RESUME\r\n│Enter confirms/retries".encode("utf-8")
        for split in range(len(initial) + 1):
            screen = module.Utf8Screen(3, 60)
            screen.feed(initial[:split])
            screen.feed(initial[split:])
            screen.feed(b"\x1b[1;10HPAUSE ")
            self.assertIn("CONFIRM PAUSE", str(screen))
            self.assertIn("Enter confirms/retries", str(screen))


if __name__ == "__main__":
    unittest.main()
