#!/usr/bin/env python3
"""Compare the production translation scan with its per-file GNU baseline.

Fixtures and helper instrumentation live in temporary directories. Optional
--benchmark reports nine interleaved runs without asserting timing thresholds.
"""

import argparse
import json
import os
from pathlib import Path
import re
import statistics
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
SOURCE = (ROOT / "services/Translation.qml").read_text()
SCANNER = SOURCE.split("component TranslationScanner: Process {", 1)[1]
COMMAND = re.search(r"command:\s*(\[[^\n]+\])", SCANNER).group(1)
COMMAND = json.loads(COMMAND.replace("translationScanner.translationsDir", '"TRANSLATIONS_DIR"'))


def scan_command(directory, baseline=False, helper=None):
    command = [str(directory) if arg == "TRANSLATIONS_DIR" else arg for arg in COMMAND]
    if baseline:
        command[command.index("-exec") + 1:] = ["/usr/bin/basename", "{}", ".json", ";"]
    if helper:
        command[command.index("-exec") + 1] = str(helper)
    return command


def scan(directory, baseline=False, helper=None, env=None):
    return subprocess.run(scan_command(directory, baseline, helper), capture_output=True, env=env)


class TranslationScanTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="intoo-translation-tests-")
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.directory = self.base / "-translations with spaces"
        self.directory.mkdir()

    def assert_same_scan(self, directory):
        old, new = scan(directory, True), scan(directory)
        self.assertEqual(new.returncode, old.returncode)
        self.assertEqual(new.stdout, old.stdout)
        self.assertEqual(new.stderr, old.stderr)
        return new

    def test_repository_catalogues(self):
        result = self.assert_same_scan(ROOT / "translations")
        self.assertEqual(result.returncode, 0)
        expected = {path.stem.encode() for path in (ROOT / "translations").glob("*.json")
                    if path.is_file()}
        self.assertEqual(set(result.stdout.splitlines()), expected)

    def test_filename_and_depth_boundaries(self):
        names = ["en_US.json", "with spaces.json", "repeated.json.json", "日本語.json",
                 "-leading.json", ".json", "line\nbreak.json"]
        for name in names:
            (self.directory / name).write_text("{}")
        (self.directory / "ignored.txt").write_text("{}")
        (self.directory / "ignored.JSON").write_text("{}")
        nested = self.directory / "directory.json"
        nested.mkdir()
        (nested / "nested.json").write_text("{}")
        (self.directory / "symlink.json").symlink_to(self.directory / "en_US.json")
        result = self.assert_same_scan(self.directory)
        self.assertEqual(result.returncode, 0)
        self.assertIn(b"repeated.json\n", result.stdout)
        self.assertIn("日本語\n".encode(), result.stdout)
        self.assertNotIn(b"ignored", result.stdout)
        self.assertNotIn(b"nested", result.stdout)
        self.assertNotIn(b"directory", result.stdout)
        self.assertNotIn(b"symlink", result.stdout)

    def test_empty_directory(self):
        result = self.assert_same_scan(self.directory)
        self.assertEqual((result.returncode, result.stdout), (0, b""))

    def test_missing_directory(self):
        result = self.assert_same_scan(self.base / "missing")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, b"")
        # The existing Process handler still supplies the English fallback.
        self.assertRegex(SCANNER, r'if \(exitCode !== 0\)\s*\{\s*'
                         r'translationScanner\.languagesScanned\(\["en_US"\]\);')

    def test_helper_process_count(self):
        for number in range(18):
            (self.directory / f"locale_{number:02}.json").write_text("{}")
        helper = self.base / "basename-wrapper"
        helper.write_text(f"#!{sys.executable}\n"
                          "import os, sys\n"
                          "with open(os.environ['TRANSLATION_HELPER_LOG'], 'a') as output:\n"
                          "    output.write('invoked\\n')\n"
                          "os.execv('/usr/bin/basename', ['basename', *sys.argv[1:]])\n")
        helper.chmod(0o755)
        log = self.base / "helper.log"
        env = dict(os.environ, TRANSLATION_HELPER_LOG=str(log))
        outputs = []
        for baseline, expected in [(True, 18), (False, 1)]:
            log.write_text("")
            result = scan(self.directory, baseline, helper, env)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(len(log.read_text().splitlines()), expected)
            outputs.append(result.stdout)
        self.assertEqual(outputs[0], outputs[1])


def benchmark():
    directory = ROOT / "translations"
    timings = {True: [], False: []}
    reference = scan(directory, True)
    for run in range(9):
        for baseline in ([True, False] if run % 2 == 0 else [False, True]):
            start = time.perf_counter()
            result = scan(directory, baseline)
            timings[baseline].append((time.perf_counter() - start) * 1000)
            if (result.returncode, result.stdout, result.stderr) != (
                    reference.returncode, reference.stdout, reference.stderr):
                raise AssertionError("benchmark scan differs from baseline")
    print(f"Translation scan: {len(reference.stdout.splitlines())} catalogues; "
          f"9 interleaved runs, median baseline {statistics.median(timings[True]):.3f} ms, "
          f"batched {statistics.median(timings[False]):.3f} ms")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--benchmark", action="store_true")
    args = parser.parse_args()
    result = unittest.TextTestRunner(verbosity=2).run(
        unittest.defaultTestLoader.loadTestsFromTestCase(TranslationScanTests))
    if not result.wasSuccessful():
        sys.exit(1)
    if args.benchmark:
        benchmark()
