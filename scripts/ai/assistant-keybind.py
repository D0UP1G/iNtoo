#!/usr/bin/env python3
"""Add the assistant binding only when Super+S is free in the Niri include tree."""

import argparse
import glob
import os
from pathlib import Path
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from niri_kdl import include_path, nodes


def bind_target(config, seen=None):
    seen = set() if seen is None else seen
    config = config.resolve()
    if config in seen:
        return None
    seen.add(config)
    text = config.read_text()
    target = None
    for node in nodes(text):
        if node.disabled:
            continue
        parts = node.name.lower().split("+")
        if len(parts) == 2 and parts[-1] == "s" and parts[0] in ("mod", "super", "win"):
            raise ValueError("Super+S is already bound; keeping the existing shortcut")
        if node.name == "binds":
            if target is None:
                target = config
        included_path = include_path(node)
        if included_path:
            included = Path(included_path).expanduser()
            if not included.is_absolute():
                included = config.parent / included
            for match in sorted(glob.glob(str(included))):
                candidate = bind_target(Path(match), seen)
                if candidate is not None:
                    target = candidate
    return target


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("config", type=Path)
    args = parser.parse_args()
    try:
        target = bind_target(args.config)
        if target is None:
            return 1
        if args.check:
            return 0
        # Append a separate binds block; Niri merges these and the parser above
        # checks every included block before changing anything.
        text = target.read_text()
        addition = '\n// iNtoo desktop companion\nbinds {\n    Mod+S repeat=false { spawn "inir" "assistant" "toggle"; }\n}\n'
        fd, temporary = tempfile.mkstemp(prefix=target.name + ".", dir=target.parent)
        try:
            with os.fdopen(fd, "w") as stream:
                stream.write(text + addition)
            os.chmod(temporary, target.stat().st_mode & 0o777)
            os.replace(temporary, target)
        finally:
            if os.path.exists(temporary):
                os.unlink(temporary)
    except (OSError, ValueError) as error:
        if not args.check:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
