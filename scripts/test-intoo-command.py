#!/usr/bin/env python3
"""Regression checks for the canonical CLI and compatibility migration."""

import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts/lib"))
SPEC = importlib.util.spec_from_file_location("rename_launcher", ROOT / "scripts/lib/rename-launcher.py")
rename_launcher = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(rename_launcher)


class IntooCommandTests(unittest.TestCase):
    def test_help_uses_canonical_command_and_legacy_wrapper_matches(self):
        outputs = []
        for command in ("intoo", "inir"):
            result = subprocess.run([str(ROOT / "scripts" / command), "help"],
                                    capture_output=True, text=True, check=True)
            outputs.append(result.stdout)
        self.assertEqual(*outputs)
        self.assertIn("Usage: intoo <command>", outputs[0])
        self.assertNotIn("Usage: inir", outputs[0])

    def test_legacy_wrapper_preserves_arguments_and_exit_status(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            wrapper = target / "inir"
            wrapper.write_bytes((ROOT / "scripts/inir").read_bytes())
            wrapper.chmod(0o755)
            executable = target / "intoo"
            executable.write_text('#!/bin/sh\nprintf "%s\\n" "$@"\nexit 7\n')
            executable.chmod(0o755)
            result = subprocess.run([str(wrapper), "assistant", "auth", "with spaces", "*"],
                                    capture_output=True, text=True)
            self.assertEqual(result.returncode, 7)
            self.assertEqual(result.stdout.splitlines(), ["assistant", "auth", "with spaces", "*"])

    def test_packaged_launchers_include_both_names(self):
        with tempfile.TemporaryDirectory() as directory:
            subprocess.run(["make", "install-bin", f"PREFIX={directory}"], cwd=ROOT, check=True)
            for command in ("intoo", "inir"):
                executable = Path(directory) / "bin" / command
                self.assertTrue(os.access(executable, os.X_OK))
                self.assertEqual(executable.read_bytes(), (ROOT / "scripts" / command).read_bytes())

    def test_completions_target_intoo(self):
        for shell, marker in (("bash", "complete -F _intoo_completions intoo"),
                              ("zsh", "#compdef intoo"), ("fish", "complete -c intoo")):
            result = subprocess.run([str(ROOT / "scripts/intoo"), "completions", shell],
                                    capture_output=True, text=True, check=True)
            self.assertIn(marker, result.stdout)
        with tempfile.TemporaryDirectory() as directory:
            env = dict(os.environ, XDG_DATA_HOME=str(Path(directory) / "data"),
                       XDG_CONFIG_HOME=str(Path(directory) / "config"))
            for shell, relative in (("bash", "data/bash-completion/completions/intoo"),
                                    ("zsh", "data/zsh/site-functions/_intoo"),
                                    ("fish", "config/fish/completions/intoo.fish")):
                subprocess.run([str(ROOT / "scripts/intoo"), "completions", "install", shell],
                               env=env, capture_output=True, check=True)
                self.assertEqual((Path(directory) / relative).resolve(),
                                 ROOT / "scripts/completions" / f"intoo.{shell}")

    def test_only_direct_legacy_commands_are_renamed(self):
        before = ('// spawn "inir" "settings"\n'
                  'binds { Mod+S { spawn "inir" "assistant" "toggle"; } }\n'
                  'spawn-at-startup "/home/a b/.local/bin/inir" "run" "--session"\n'
                  '/-spawn-at-startup "inir" "start"\n'
                  'spawn-at-startup "qs" "-c" "inir"\n'
                  'spawn-sh-at-startup "inir run"\n'
                  'window-rule { match app-id="inir"; }\n')
        after = rename_launcher.rewrite(before)
        self.assertEqual(after, before.replace('spawn "inir" "assistant"', 'spawn "intoo" "assistant"')
                         .replace('/home/a b/.local/bin/inir', '/home/a b/.local/bin/intoo'))
        self.assertEqual(rename_launcher.rewrite(after), after)

    def test_migration_follows_includes_and_is_idempotent(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            niri = home / "niri"
            niri.mkdir()
            config = niri / "config.kdl"
            config.write_text('include "binds.kdl"\nspawn-at-startup "inir" "run" "--session"\n')
            binds = niri / "binds.kdl"
            binds.write_text('binds {\n    Mod+S { spawn "inir" "assistant" "toggle"; }\n}\n')
            binds.chmod(0o640)
            self.assertTrue(rename_launcher.rename(home, check=True))
            self.assertIn('"inir"', binds.read_text())
            self.assertTrue(rename_launcher.rename(home))
            self.assertIn('"intoo"', binds.read_text())
            self.assertIn('"intoo"', config.read_text())
            self.assertEqual(binds.stat().st_mode & 0o777, 0o640)
            self.assertFalse(rename_launcher.rename(home, check=True))
            self.assertFalse(rename_launcher.rename(home))

    def test_validation_failure_publishes_no_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            (home / "niri").mkdir()
            config = home / "niri/config.kdl"
            original = 'spawn-at-startup "inir" "run" "--session"\n'
            config.write_text(original)
            with patch.object(rename_launcher.startup, "_validate_candidate", side_effect=ValueError("invalid")):
                with self.assertRaises(ValueError):
                    rename_launcher.rename(home)
            self.assertEqual(config.read_text(), original)

    def test_session_startup_recognizes_both_commands(self):
        from niri_kdl import nodes
        for command in ("intoo", "inir", "/home/a/.local/bin/intoo", "/home/a/.local/bin/inir"):
            node = next(nodes(f'spawn-at-startup "{command}" "run" "--session"\n'))
            self.assertTrue(rename_launcher.startup.SHELL.match(node))

    def test_keybind_descriptions_recognize_both_commands(self):
        spec = importlib.util.spec_from_file_location("keybinds", ROOT / "scripts/parse_niri_keybinds.py")
        parser = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(parser)
        for command in ("intoo", "inir", "/home/a/.local/bin/intoo"):
            action = f'spawn "{command}" "assistant" "toggle";'
            self.assertEqual(parser.parse_inir_action(action), ("assistant", "toggle"))


if __name__ == "__main__":
    unittest.main()
