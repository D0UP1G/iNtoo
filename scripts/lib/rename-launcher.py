#!/usr/bin/env python3
"""Rename direct inir commands in active Niri includes, preserving other settings."""

import argparse
import importlib.util
import json
from pathlib import Path
import sys

from niri_kdl import nodes

_SPEC = importlib.util.spec_from_file_location("session_startup", Path(__file__).with_name("session-startup.py"))
startup = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(startup)


def rewrite(text):
    edits = []
    for node in nodes(text):
        if node.disabled or node.name not in ("spawn", "spawn-at-startup"):
            continue
        args = [token for token in node.args if not token.disabled]
        if not args or args[0].kind != "string" or Path(args[0].value).name != "inir":
            continue
        token = args[0]
        command = str(Path(token.value).with_name("intoo"))
        edits.append((token.start, token.end, json.dumps(command, ensure_ascii=False)))
    for start, end, command in reversed(edits):
        text = text[:start] + command + text[end:]
    return text


def rename(config_home, check=False):
    config_home = Path(config_home)
    root = config_home / "niri/config.kdl"
    if not root.is_file():
        return False
    changed = {}
    for path in startup.config_files(root):
        original = path.read_text(encoding="utf-8")
        updated = rewrite(original)
        if updated != original:
            changed[path] = updated
    if changed and not check:
        startup._validate_candidate(config_home, root, changed)
        startup.publish_transaction(changed)
    return bool(changed)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--config-home", type=Path, required=True)
    args = parser.parse_args()
    try:
        changed = rename(args.config_home, args.check)
        return int(args.check and not changed)
    except (OSError, ValueError) as error:
        print(error, file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
