#!/usr/bin/env python3
"""Install missing iNToo Niri binds and make every launcher path explicit."""

import glob
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
NIRI_CONFIG_HELPER = SCRIPT_DIR / "niri-config.py"
KEYBIND_PARSER = SCRIPT_DIR / "parse_niri_keybinds.py"
MANAGED_RELATIVE = Path("config.d/75-intoo-binds.kdl")
MANAGED_INCLUDE = 'include "config.d/75-intoo-binds.kdl"'


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Could not load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def active_bind_blocks(content: str, parser):
    normal, recent = parser.find_all_binds_blocks(content)
    return [*normal, *recent]


def bind_key(bind: dict):
    return tuple(bind["mods"]), bind["key"]


def launcher_rewrite(content: str, launcher: str) -> str:
    spawn = re.compile(r'(\bspawn\s+)("(?:\\.|[^"\\])*")')
    output = []
    for line in content.splitlines(keepends=True):
        if line.lstrip().startswith("//"):
            output.append(line)
            continue

        def replace(match):
            try:
                current = json.loads(match.group(2))
            except json.JSONDecodeError:
                return match.group(0)
            if current == "inir" or (current.endswith("/inir") and "/" in current):
                return match.group(1) + json.dumps(launcher, ensure_ascii=False)
            return match.group(0)

        output.append(spawn.sub(replace, line))
    return "".join(output)


def install_include(root: Path) -> bool:
    content = root.read_text()
    if re.search(r'^\s*include\s+"config\.d/75-intoo-binds\.kdl"\s*$', content, re.M):
        return False

    # Respect an existing wildcard include that already picks up the managed file.
    for match in re.finditer(r'^\s*include\s+"([^"]+)"\s*$', content, re.M):
        pattern = os.path.expandvars(os.path.expanduser(match.group(1)))
        if not os.path.isabs(pattern):
            pattern = str(root.parent / pattern)
        if any(Path(candidate).resolve() == (root.parent / MANAGED_RELATIVE).resolve()
               for candidate in glob.glob(pattern)):
            return False

    include_90 = re.search(r'^\s*include\s+"config\.d/90-user-extra\.kdl"\s*$', content, re.M)
    if include_90:
        pos = include_90.start()
        content = content[:pos] + MANAGED_INCLUDE + "\n" + content[pos:]
    else:
        include_70 = re.search(r'^\s*include\s+"config\.d/70-binds\.kdl"\s*$', content, re.M)
        if include_70:
            pos = include_70.end()
            content = content[:pos] + "\n" + MANAGED_INCLUDE + content[pos:]
        else:
            content = content.rstrip() + "\n\n" + MANAGED_INCLUDE + "\n"
    root.write_text(content)
    return True


def rollback(originals: dict[Path, str]) -> None:
    for path, content in originals.items():
        path.write_text(content)


def main() -> int:
    if len(sys.argv) != 3:
        print("Usage: sync-niri-binds.py LAUNCHER CONFIG_HOME", file=sys.stderr)
        return 2

    launcher = str(Path(sys.argv[1]).expanduser().resolve())
    config_home = Path(sys.argv[2]).expanduser()
    niri_dir = config_home / "niri"
    root = niri_dir / "config.kdl"
    if not root.is_file():
        return 0

    defaults = SCRIPT_DIR.parent / "defaults/niri/config.d/70-binds.kdl"
    if not defaults.is_file():
        print(f"Missing shipped Niri binds: {defaults}", file=sys.stderr)
        return 1

    niri_config = load_module("inir_niri_config", NIRI_CONFIG_HELPER)
    parser = load_module("inir_niri_keybind_parser", KEYBIND_PARSER)
    managed = niri_dir / MANAGED_RELATIVE
    managed.parent.mkdir(parents=True, exist_ok=True)

    paths = [root, *sorted((niri_dir / "config.d").glob("*.kdl"))]
    originals = {path: path.read_text() for path in paths if path.is_file()}
    originals.setdefault(managed, managed.read_text() if managed.is_file() else "")
    rewrote = False
    for path in paths:
        if not path.is_file():
            continue
        updated = launcher_rewrite(path.read_text(), launcher)
        if updated != path.read_text():
            path.write_text(updated)
            rewrote = True

    # Ignore the generated file while checking existing user binds, so the
    # managed defaults can be refreshed on every update without duplicating.
    flattened = niri_config._flatten_niri_config(root, {managed.resolve()})
    occupied = {
        bind_key(bind)
        for block in active_bind_blocks(flattened, parser)
        for bind in parser.parse_keybinds_from_block(block)
    }

    defaults_text = defaults.read_text()
    default_entries = []
    for line in defaults_text.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("//") or 'spawn "inir"' not in stripped:
            continue
        parsed = parser.parse_keybinds_from_block("\n" + line + "\n")
        if parsed:
            default_entries.append((bind_key(parsed[0]), line.strip()))

    missing = []
    for combo, line in default_entries:
        if combo in occupied:
            continue
        absolute = re.sub(r'\bspawn\s+"inir"',
                          "spawn " + json.dumps(launcher, ensure_ascii=False), line, count=1)
        missing.append("    " + absolute)
        occupied.add(combo)

    managed_content = (
        "// Managed by iNToo setup. Local key collisions are kept in the user's config.\n"
        "binds {\n" + "\n".join(missing) + "\n}\n"
    )
    old_managed = managed.read_text() if managed.exists() else ""
    if old_managed != managed_content:
        managed.write_text(managed_content)
        rewrote = True
    if install_include(root):
        originals.setdefault(root, originals.get(root, ""))
        rewrote = True

    if rewrote and shutil.which("niri"):
        result = subprocess.run(["niri", "validate", "-c", str(root)],
                                text=True, capture_output=True)
        if result.returncode != 0:
            rollback(originals)
            if not originals[managed]:
                managed.unlink(missing_ok=True)
            print("Niri rejected the iNToo bind update:", file=sys.stderr)
            print((result.stdout + result.stderr).strip(), file=sys.stderr)
            return 1

    print(f"Synchronized iNToo keybinds in {managed}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
