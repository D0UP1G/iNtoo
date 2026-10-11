#!/usr/bin/env python3
"""Contract checks for the assistant's default provider and API selection.

The resolver lives inside the Ai singleton, whose construction pulls in most of
the shell, so its ordering is checked against the real source. The shipped
provider presets are additionally loaded through Quickshell itself, which
verifies the data the resolver consumes.
"""

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
AI = ROOT / "services/Ai.qml"
CONFIG = ROOT / "modules/common/Config.qml"
PRESETS = ROOT / "modules/common/AiProviderPresets.qml"
DEFAULTS = ROOT / "defaults/config.json"
II_SETTINGS = ROOT / "modules/settings/AiConfig.qml"
WAFFLE_SETTINGS = ROOT / "modules/waffle/settings/pages/WAiPage.qml"
MODEL_SELECTOR = ROOT / "modules/sidebarLeft/aiChat/AiModelSelector.qml"

PROBE_QML = '''import QtQuick
import Quickshell
import qs.modules.common

ShellRoot {
    Timer {
        interval: 200
        running: true
        onTriggered: {
            const deepseek = AiProviderPresets.byId("deepseek")
            console.log("PRESET_PROBE " + JSON.stringify({
                count: AiProviderPresets.presets.length,
                ids: AiProviderPresets.presets.map(preset => preset.id),
                deepseek: deepseek,
                entry: AiProviderPresets.toModelEntry(deepseek),
                missing: AiProviderPresets.byId("does-not-exist")
            }))
        }
    }
}
'''


def read(path):
    return Path(path).read_text()


def resolver_body():
    match = re.search(r"function _resolvedCurrentModelId\(\): string \{(.*?)\n    \}", read(AI), re.S)
    if not match:
        raise AssertionError("services/Ai.qml no longer defines _resolvedCurrentModelId")
    return match.group(1)


class DefaultProviderContract(unittest.TestCase):
    def test_schema_declares_the_default_provider_keys(self):
        schema = read(CONFIG)
        for key in ("defaultProvider", "defaultModel", "defaultApiFormat"):
            self.assertIn(f'property string {key}:', schema,
                          f"Config.qml ai schema lacks {key}")

    def test_shipped_defaults_carry_empty_provider_keys(self):
        config = json.loads(read(DEFAULTS))
        for key in ("defaultProvider", "defaultModel", "defaultApiFormat"):
            self.assertIn(key, config["ai"], f"defaults/config.json ai lacks {key}")
        self.assertEqual(config["ai"]["defaultProvider"], "")
        self.assertEqual(config["ai"]["defaultModel"], "")

    def test_shipped_model_rows_are_tagged_and_deduplicated(self):
        config = json.loads(read(DEFAULTS))
        rows = config["ai"]["extraModels"]
        self.assertTrue(rows, "the shipped profile should keep curated fallback rows")
        models = [row.get("model") for row in rows]
        self.assertNotIn("gemini-2.5-flash", models,
                         "a superseded duplicate row should not ship again")
        for row in rows:
            self.assertTrue(row.get("provider_id"),
                            f"row {row.get('model')} must name its curated provider")

    def test_resolver_prefers_the_explicit_choice_then_the_default_provider(self):
        body = resolver_body()
        anchors = [
            "Persistent.states?.ai?.model",
            "_defaultProviderModelId()",
            'recommendedModelIds("auto", 1)',
            "runnableModelList[0]",
        ]
        positions = []
        for anchor in anchors:
            self.assertIn(anchor, body, f"resolver no longer consults {anchor}")
            positions.append(body.index(anchor))
        self.assertEqual(positions, sorted(positions),
                         "resolver priority changed: explicit choice, default provider, heuristics")

    def test_resolver_never_falls_back_to_a_locked_model(self):
        body = resolver_body()
        self.assertNotIn("root.modelList[0]", body,
                         "the resolver must not return a model that cannot run")
        self.assertIn('return ""', body)

    def test_deepseek_is_a_curated_preset(self):
        presets = read(PRESETS)
        self.assertIn('"id": "deepseek"', presets)
        block = presets.split('"id": "deepseek"', 1)[1].split("},", 1)[0]
        for expected in ('"endpoint": "https://api.deepseek.com/chat/completions"',
                         '"api_format": "openai"',
                         '"keyId": "deepseek"',
                         '"catalogKind": "openai-compatible"',
                         '"modelsEndpoint": "https://api.deepseek.com/models"'):
            self.assertIn(expected, block, f"deepseek preset lacks {expected}")
        self.assertTrue((ROOT / "assets/icons/deepseek-symbolic.svg").is_file(),
                        "the preset icon must ship with the shell")

    def test_both_families_expose_the_default_controls(self):
        for path in (II_SETTINGS, WAFFLE_SETTINGS):
            source = read(path)
            self.assertIn("Ai.setDefaultProvider", source, f"{path} cannot set a default")
            self.assertIn("Ai.clearDefaultProvider", source, f"{path} cannot clear a default")
            self.assertIn("defaultProvider", source, f"{path} does not show the default")
        self.assertIn("Ai.setDefaultProvider", read(MODEL_SELECTOR))
        self.assertIn("isDefault: Ai.isDefaultModelId(id)", read(MODEL_SELECTOR))


class PresetRuntime(unittest.TestCase):
    def test_presets_load_in_native_quickshell(self):
        qs = shutil.which("qs")
        if not qs or not shutil.which("dbus-run-session") or not shutil.which("timeout"):
            self.skipTest("native probe requires qs, dbus-run-session and timeout")
        with tempfile.TemporaryDirectory() as temporary:
            probe = Path(temporary) / "probe"
            (probe / "modules/common").mkdir(parents=True)
            (probe / "services").mkdir(parents=True)
            (probe / "qmldir").write_text("module qs\n")
            (probe / "modules/common/qmldir").write_text(
                "module qs.modules.common\n"
                "singleton AiProviderPresets 1.0 AiProviderPresets.qml\n")
            shutil.copy(PRESETS, probe / "modules/common/AiProviderPresets.qml")
            (probe / "services/qmldir").write_text(
                "module qs.services\nsingleton Translation 1.0 Translation.qml\n")
            (probe / "services/Translation.qml").write_text(
                "pragma Singleton\nimport QtQuick\n"
                "QtObject { function tr(text) { return String(text ?? \"\") } }\n")
            (probe / "shell.qml").write_text(PROBE_QML)

            environment = {**os.environ, "QT_QPA_PLATFORM": "offscreen",
                           "QS_DISABLE_CRASH_HANDLER": "1", "INIR_DISABLE_HOT_RELOAD": "1"}
            for name in ("DBUS_SESSION_BUS_ADDRESS", "NIRI_SOCKET", "WAYLAND_DISPLAY",
                         "HYPRLAND_INSTANCE_SIGNATURE"):
                environment.pop(name, None)
            result = subprocess.run(
                ["dbus-run-session", "--", "timeout", "--kill-after=1s", "3s",
                 qs, "-n", "-p", str(probe)],
                env=environment, text=True, capture_output=True, timeout=7)
            log = result.stdout + result.stderr
            self.assertIn("Configuration Loaded", log, log)
            self.assertNotIn("TypeError", log, log)
            self.assertIn("PRESET_PROBE ", log, log)
            payload = json.loads(log.split("PRESET_PROBE ", 1)[1].splitlines()[0])

        self.assertGreater(payload["count"], 10)
        self.assertIn("deepseek", payload["ids"])
        self.assertEqual(payload["deepseek"]["endpoint"],
                         "https://api.deepseek.com/chat/completions")
        self.assertEqual(payload["deepseek"]["api_format"], "openai")
        self.assertTrue(payload["deepseek"]["requiresKey"])
        self.assertEqual(payload["entry"]["provider_id"], "deepseek")
        self.assertEqual(payload["entry"]["auth_scheme"], "bearer")
        self.assertEqual(payload["entry"]["model"], "deepseek-chat")
        self.assertIsNone(payload["missing"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
